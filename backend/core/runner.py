"""Background experiment execution.

Two execution modes:

* **Live** — when the `agent-workbench` binary and an LLM API key are available,
  spawn the Rust `exec` subcommand and stream real kernel `AgentEvent`s.
* **Dry-run** — otherwise, build the harness via `agent-workbench run` and report
  the assembled prompt/summary so the platform stays usable without credentials.
"""

import asyncio
import time
from typing import Dict, Literal, Optional

from backend.core.engine import missing_requirements, stream_exec
from backend.core.manager import ExperimentManager
from backend.core.rust_validator import dry_run_with_rust
from backend.models.schemas import AgentEvent, ExperimentConfig

EventType = Literal["thought", "action", "observation", "error", "complete"]

# Maps kernel AgentEvent tags (serde snake_case) onto the platform's SSE types.
_KERNEL_EVENT_MAP: Dict[str, EventType] = {
    "text_delta": "thought",
    "text_segment_start": "thought",
    "tool_start": "action",
    "tool_end": "observation",
    "permission_request": "action",
    "error": "error",
    "run_start": "thought",
}


async def _emit(
    manager: ExperimentManager,
    experiment_id: str,
    event_type: EventType,
    content: str,
    metadata: Optional[Dict] = None,
):
    await manager.push_event(
        experiment_id,
        AgentEvent(
            timestamp=time.time(),
            type=event_type,
            content=content,
            metadata=metadata,
        ),
    )


def _format_kernel_event(event: Dict) -> Optional[tuple]:
    """Translate a kernel NDJSON event into (event_type, content, metadata).

    Returns None for events that should not be surfaced to the UI directly
    (e.g. terminal `done` / `run_complete`, handled by the caller).
    """
    kind = event.get("type", "")

    if kind in ("done", "run_complete", "cancelled"):
        return None

    if kind == "text_delta":
        return ("thought", event.get("delta", ""), None)

    if kind == "text_segment_start":
        return ("thought", f"[turn {event.get('iteration', '?')}]", None)

    if kind == "tool_start":
        name = event.get("name", "tool")
        tool_input = event.get("input")
        return ("action", f"{name}({tool_input})", {"tool_name": name, "input": tool_input})

    if kind == "tool_end":
        name = event.get("name", "tool")
        result = event.get("result", "")
        is_error = bool(event.get("is_error"))
        event_type: EventType = "error" if is_error else "observation"
        return (event_type, f"[{name}] {result}", {"tool_name": name, "is_error": is_error})

    if kind == "permission_request":
        return (
            "action",
            f"Permission required: {event.get('tool_name', '?')} — {event.get('description', '')}",
            {"request_id": event.get("request_id")},
        )

    if kind == "run_start":
        return ("thought", f"Live run started (model={event.get('model', '?')})", None)

    if kind == "error":
        return ("error", event.get("message", "unknown error"), None)

    mapped = _KERNEL_EVENT_MAP.get(kind)
    if mapped:
        return (mapped, json_safe(event), None)
    return None


def json_safe(event: Dict) -> str:
    content = event.get("content") or event.get("message") or event.get("delta")
    return str(content) if content is not None else event.get("type", "")


async def _run_live(
    manager: ExperimentManager,
    experiment_id: str,
    config_yaml: str,
    prompt: str,
    workspace_path: Optional[str],
) -> bool:
    """Stream a real kernel run. Returns True on success."""
    saw_error = False
    async for event in stream_exec(config_yaml, prompt, workspace_path):
        formatted = _format_kernel_event(event)
        if formatted is None:
            if event.get("type") == "error":
                saw_error = True
            continue
        event_type, content, metadata = formatted
        if event_type == "error":
            saw_error = True
        if content:
            await _emit(manager, experiment_id, event_type, content, metadata)
    return not saw_error


DEFAULT_PROMPT = (
    "Briefly introduce yourself and confirm which tools you have available. "
    "Keep the response under 5 sentences."
)


async def _run_dry(
    manager: ExperimentManager,
    experiment_id: str,
    config: ExperimentConfig,
    config_yaml: str,
    workspace_path: Optional[str],
) -> bool:
    """Fall back to a dry-run harness build when no live engine is available."""
    reason = missing_requirements()
    if reason:
        await _emit(manager, experiment_id, "thought", f"Live engine unavailable ({reason}). Falling back to dry-run.")

    agent_name = config.agent.get("name", "Agent")
    strategy = "unknown"
    if isinstance(config.strategy, dict):
        strategy = str(config.strategy.get("type", "unknown"))

    await _emit(
        manager,
        experiment_id,
        "action",
        f"validate_config(agent={agent_name}, strategy={strategy})",
    )

    ok, errors, lines = await dry_run_with_rust(config_yaml, workspace_path)
    if not ok:
        for err in errors:
            await _emit(manager, experiment_id, "error", err)
        return False

    await _emit(manager, experiment_id, "observation", "Rust harness built successfully.")
    for line in lines:
        is_meta = line.startswith("---") or line.startswith("Note:")
        await _emit(manager, experiment_id, "thought" if is_meta else "observation", line)
        await asyncio.sleep(0.02)

    await _emit(manager, experiment_id, "thought", "Dry-run complete (no live LLM call).")
    return True


async def run_experiment(
    manager: ExperimentManager,
    experiment_id: str,
    config: ExperimentConfig,
    config_yaml: str,
    workspace_path: Optional[str] = None,
    prompt: Optional[str] = None,
):
    """Execute an experiment asynchronously and stream events."""
    prompt = (prompt or "").strip() or DEFAULT_PROMPT
    try:
        if missing_requirements() is None:
            success = await _run_live(manager, experiment_id, config_yaml, prompt, workspace_path)
        else:
            success = await _run_dry(manager, experiment_id, config, config_yaml, workspace_path)

        await manager.complete_experiment(
            experiment_id,
            success=success,
            error_message=None if success else "Experiment finished with errors",
        )
    except Exception as exc:  # pragma: no cover - safety net for background task
        await _emit(manager, experiment_id, "error", str(exc))
        await manager.complete_experiment(experiment_id, success=False, error_message=str(exc))


def schedule_experiment(
    manager: ExperimentManager,
    experiment_id: str,
    config: ExperimentConfig,
    config_yaml: str,
    workspace_path: Optional[str] = None,
    prompt: Optional[str] = None,
):
    """Fire-and-forget background task for an experiment."""
    asyncio.create_task(
        run_experiment(manager, experiment_id, config, config_yaml, workspace_path, prompt)
    )
