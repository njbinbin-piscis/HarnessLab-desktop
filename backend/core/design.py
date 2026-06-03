"""The AI experiment-design assistant (Architect) orchestration (Pillar 2).

Turns a natural-language requirement into a runnable agent config. Unlike the
deterministic :mod:`backend.core.meta_generator` (which fills a template), this
drives the **Architect agent** (``configs/meta_agent.yaml``) as a live workbench
session so the assistant itself is a designed, pluggable agent.

Degradation: when no live engine is available (no binary / API key), the flow
falls back to the deterministic generator so the platform stays usable. The
session factory and engine check are injected so the agent path is testable with
a fake session.
"""

from __future__ import annotations

import re
from pathlib import Path
from typing import Any, Callable, Dict, Optional

from backend.core.engine import InteractiveSession, engine_available
from backend.models.schemas import MetaAgentGenerateRequest

WORKBENCH_ROOT = Path(__file__).resolve().parents[2]
ARCHITECT_CONFIG = WORKBENCH_ROOT / "configs" / "meta_agent.yaml"

_YAML_BLOCK = re.compile(r"```(?:yaml|yml)?\s*\n(.*?)```", re.DOTALL)

# A factory returning a fresh session-like object (InteractiveSession-compatible).
SessionFactory = Callable[[], Any]
EngineCheck = Callable[[], bool]


def read_architect_config() -> str:
    """Return the Architect's own YAML config (the designable design assistant)."""
    return ARCHITECT_CONFIG.read_text(encoding="utf-8")


def extract_yaml(text: str) -> Optional[str]:
    """Pull the first fenced YAML block from the assistant's response, if any."""
    m = _YAML_BLOCK.search(text)
    if m:
        return m.group(1).strip()
    # Fall back to a heuristic: a block that starts at an ``agent:`` line.
    idx = text.find("agent:")
    if idx != -1:
        return text[idx:].strip()
    return None


def _architect_task_prompt(req: MetaAgentGenerateRequest) -> str:
    tools = ", ".join(req.allowed_tools) or "general tools"
    return (
        "Design a new agent for the following requirement and output ONLY a "
        "single fenced ```yaml block containing the complete workbench config "
        "(agent / plugins / policy / compression / strategy / hooks).\n\n"
        f"Requirement: {req.goal_description.strip()}\n"
        f"Preferred model: {req.model_preference}\n"
        f"Loop strategy: {req.strategy_type}\n"
        f"Allowed tools: {tools}\n"
    )


async def _collect_turn_text(session: Any, prompt: str, ready_timeout: float = 15.0) -> str:
    """Drive one Architect turn and return the assembled assistant text."""
    await session.start()
    await session.send_prompt(prompt)
    parts = []
    try:
        async for event in session.events():
            etype = event.get("type")
            if etype == "text_delta":
                parts.append(event.get("delta", ""))
            elif etype in ("turn_complete", "done", "run_complete"):
                break
            elif etype in ("error", "agent_closed"):
                break
    finally:
        await session.close()
    return "".join(parts).strip()


def _template_fallback(req: MetaAgentGenerateRequest) -> Dict[str, Any]:
    from backend.core.meta_generator import generate_agent_config

    config_yaml, system_prompt, validation = generate_agent_config(req)
    return {
        "source": "template",
        "config_yaml": config_yaml,
        "system_prompt": system_prompt,
        "validation": {
            "valid": validation.valid,
            "errors": validation.errors,
            "warnings": validation.warnings,
        },
    }


async def generate_design(
    req: MetaAgentGenerateRequest,
    *,
    engine_check: Optional[EngineCheck] = None,
    session_factory: Optional[SessionFactory] = None,
    workspace: Optional[str] = None,
) -> Dict[str, Any]:
    """Generate an agent config, preferring the live Architect agent.

    Returns a dict with ``source`` ("agent" | "template"), ``config_yaml``,
    ``system_prompt``, and a ``validation`` block.
    """
    engine_check = engine_check or engine_available

    if not engine_check():
        return _template_fallback(req)

    if session_factory is None:
        config_yaml = read_architect_config()
        session_factory = lambda: InteractiveSession(config_yaml, workspace)  # noqa: E731

    try:
        session = session_factory()
        text = await _collect_turn_text(session, _architect_task_prompt(req))
    except Exception as exc:  # noqa: BLE001 - any live failure → safe fallback
        result = _template_fallback(req)
        result["agent_error"] = str(exc)
        return result

    yaml_text = extract_yaml(text)
    if not yaml_text:
        result = _template_fallback(req)
        result["agent_error"] = "architect produced no parsable YAML"
        return result

    # Validate the agent-produced YAML against the Rust schema (best-effort).
    from backend.api.config import validate_config_dict
    import yaml as _yaml

    validation_block = {"valid": True, "errors": [], "warnings": []}
    try:
        parsed = _yaml.safe_load(yaml_text) or {}
        if isinstance(parsed, dict):
            v = validate_config_dict(parsed)
            validation_block = {"valid": v.valid, "errors": v.errors, "warnings": v.warnings}
    except _yaml.YAMLError as exc:
        validation_block = {"valid": False, "errors": [f"YAML parse error: {exc}"], "warnings": []}

    return {
        "source": "agent",
        "config_yaml": yaml_text,
        "system_prompt": "",
        "validation": validation_block,
        "raw_response": text,
    }
