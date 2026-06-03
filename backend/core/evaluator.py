"""Run an agent config against a validation set and persist the results.

This is the platform's *evaluation run*: it ties the runtime (the Rust
``exec`` agent loop, via :func:`backend.core.engine.stream_exec`) to a
validation set (:mod:`backend.core.datasets`) and a scorer
(``benchmarks.SCORERS``), capturing each task's full trajectory and score into
the persistent :class:`~backend.core.store.RunStore`.

It deliberately mirrors ``benchmarks/runner.py`` (same scorers, same multi-trial
pass^k notion) but adds per-task **trajectory capture** + **persistence** +
**progress streaming**, which the offline CLI harness does not need.

The agent stream is injected (``stream_fn``) so tests can drive the whole loop
with a fake stream — no binary or API key required.
"""

from __future__ import annotations

import asyncio
import time
from typing import Any, AsyncGenerator, Callable, Dict, List, Optional

import yaml

from backend.core.datasets import DatasetRegistry, get_dataset_registry
from backend.core.store import RunStore, TaskRecord, get_run_store
from benchmarks.scorers import SCORERS
from benchmarks.types import Task

# Signature of the agent stream: (config_yaml, prompt, workspace, tools) -> async events.
StreamFn = Callable[[str, str, Optional[str], Optional[List[str]]], AsyncGenerator[Dict[str, Any], None]]


def _tools_from_config(config_yaml: str) -> Optional[List[str]]:
    """Read an optional ``reproduction.tools`` / ``enabled_tools`` whitelist."""
    try:
        cfg = yaml.safe_load(config_yaml) or {}
    except yaml.YAMLError:
        return None
    if not isinstance(cfg, dict):
        return None
    repro = cfg.get("reproduction")
    if isinstance(repro, dict) and isinstance(repro.get("tools"), list):
        return [str(t) for t in repro["tools"]]
    if "enabled_tools" in cfg and isinstance(cfg["enabled_tools"], list):
        return [str(t) for t in cfg["enabled_tools"]]
    return None


def _default_stream_fn() -> StreamFn:
    # Imported lazily so importing the evaluator never requires the engine.
    from backend.core.engine import stream_exec

    return stream_exec


async def _collect(
    stream_fn: StreamFn,
    config_yaml: str,
    prompt: str,
    workspace: Optional[str],
    tools: Optional[List[str]],
) -> Dict[str, Any]:
    """Run one turn, returning final text, token/latency telemetry, and events."""
    text_parts: List[str] = []
    last_observation = ""
    events: List[Dict[str, Any]] = []
    in_tok = out_tok = 0
    error: Optional[str] = None
    start = time.monotonic()

    async for event in stream_fn(config_yaml, prompt, workspace, tools):
        events.append(event)
        kind = event.get("type", "")
        if kind == "text_delta":
            text_parts.append(event.get("delta", ""))
        elif kind == "tool_end":
            last_observation = event.get("result", "") or last_observation
        elif kind == "error":
            error = event.get("message", "unknown error")
        elif kind in ("run_complete", "done"):
            in_tok += int(event.get("total_input_tokens", 0) or 0)
            out_tok += int(event.get("total_output_tokens", 0) or 0)

    return {
        "final_text": "".join(text_parts).strip() or last_observation,
        "events": events,
        "input_tokens": in_tok,
        "output_tokens": out_tok,
        "latency_s": time.monotonic() - start,
        "error": error,
    }


async def evaluate(
    *,
    config_yaml: str,
    dataset_id: str,
    agent_name: str = "Agent",
    scorer_name: Optional[str] = None,
    seeds: int = 1,
    limit: Optional[int] = None,
    workspace: Optional[str] = None,
    label: Optional[str] = None,
    lineage: Optional[Dict[str, Any]] = None,
    out_queue: Optional[asyncio.Queue] = None,
    stream_fn: Optional[StreamFn] = None,
    store: Optional[RunStore] = None,
    datasets: Optional[DatasetRegistry] = None,
) -> Dict[str, Any]:
    """Evaluate ``config_yaml`` over ``dataset_id`` and persist the run.

    Returns a summary dict including the persisted ``run_id`` and aggregate
    metrics. Progress events are pushed to ``out_queue`` when provided.
    """
    store = store or get_run_store()
    datasets = datasets or get_dataset_registry()
    stream_fn = stream_fn or _default_stream_fn()

    spec = datasets.get_spec(dataset_id)
    scorer_name = scorer_name or spec.scorer
    if scorer_name not in SCORERS:
        raise ValueError(f"unknown scorer '{scorer_name}'. Known: {', '.join(SCORERS)}")
    scorer = SCORERS[scorer_name]

    tasks: List[Task] = datasets.load(dataset_id)
    if limit is not None:
        tasks = tasks[:limit]
    tools = _tools_from_config(config_yaml)

    run_id = store.create_run(
        agent_name=agent_name,
        config_yaml=config_yaml,
        dataset_id=dataset_id,
        scorer=scorer_name,
        seeds=seeds,
        num_tasks=len(tasks),
        label=label,
        lineage=lineage,
    )

    async def emit(event: Dict[str, Any]) -> None:
        if out_queue is not None:
            await out_queue.put(event)

    store.set_status(run_id, "running")
    await emit({"type": "eval_start", "run_id": run_id, "dataset_id": dataset_id,
                "scorer": scorer_name, "num_tasks": len(tasks)})

    successes = 0
    total_in = total_out = 0
    total_latency = 0.0

    try:
        for idx, task in enumerate(tasks):
            await emit({"type": "task_start", "run_id": run_id, "task_id": task.task_id,
                        "index": idx, "total": len(tasks)})

            best_score = 0.0
            task_success = False
            final_text = ""
            last_error: Optional[str] = None
            events_total = 0
            t_in = t_out = 0
            t_latency = 0.0
            best_events: List[Dict[str, Any]] = []

            for _ in range(max(1, seeds)):
                turn = await _collect(stream_fn, config_yaml, task.prompt, workspace, tools)
                events_total += len(turn["events"])
                t_in += turn["input_tokens"]
                t_out += turn["output_tokens"]
                t_latency += turn["latency_s"]
                final_text = turn["final_text"] or final_text
                last_error = turn["error"] or last_error
                ok, score = scorer(task, turn["final_text"])
                if score >= best_score:
                    best_score = score
                    best_events = turn["events"]
                if ok:
                    task_success = True

            if task_success:
                successes += 1
            total_in += t_in
            total_out += t_out
            total_latency += t_latency

            rec = TaskRecord(
                task_id=task.task_id,
                success=task_success,
                score=best_score,
                trials=max(1, seeds),
                events=events_total,
                input_tokens=t_in,
                output_tokens=t_out,
                latency_s=t_latency,
                final_text=final_text,
                error=last_error,
            )
            store.add_task_result(run_id, rec)
            store.add_trajectory(run_id, task.task_id, best_events)

            await emit({"type": "task_complete", "run_id": run_id, "task_id": task.task_id,
                        "success": task_success, "score": best_score, "index": idx})

        num = len(tasks) or 1
        accuracy = successes / num
        store.finalize_run(
            run_id,
            accuracy=accuracy,
            total_input_tokens=total_in,
            total_output_tokens=total_out,
            total_latency_s=total_latency,
        )
        summary = {
            "type": "eval_complete",
            "run_id": run_id,
            "num_tasks": len(tasks),
            "accuracy": accuracy,
            "total_input_tokens": total_in,
            "total_output_tokens": total_out,
            "total_latency_s": total_latency,
        }
        await emit(summary)
        return summary
    except Exception as exc:  # pragma: no cover - safety net
        store.finalize_run(
            run_id, accuracy=0.0, total_input_tokens=total_in,
            total_output_tokens=total_out, total_latency_s=total_latency,
            status="failed", error=str(exc),
        )
        await emit({"type": "error", "run_id": run_id, "message": str(exc)})
        raise
