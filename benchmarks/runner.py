"""Execution + scoring loop for reproduction arms.

Runs each task through the Rust ``agent-workbench exec`` path (via
``backend.core.engine.stream_exec``), collects the agent's final text, and
applies the benchmark scorer. Supports multi-trial pass^k aggregation
(Reflexion success@k, tau-bench pass^k).
"""

from __future__ import annotations

import asyncio
import time
from pathlib import Path
from typing import List, Optional

import yaml

from backend.core.engine import stream_exec
from benchmarks.scorers import SCORERS
from benchmarks.types import Arm, BenchmarkResult, Scorer, Task, TaskResult

WORKBENCH_ROOT = Path(__file__).resolve().parents[1]


def _tools_from_config(config_yaml: str) -> Optional[List[str]]:
    """Read the optional ``reproduction.tools`` whitelist from a config.

    Returns None (full default registry) when unset, [] when explicitly empty
    (reasoning-only ablation), or the listed tools otherwise.
    """
    try:
        cfg = yaml.safe_load(config_yaml) or {}
    except yaml.YAMLError:
        return None
    repro = cfg.get("reproduction") if isinstance(cfg, dict) else None
    if isinstance(repro, dict) and "tools" in repro:
        tools = repro.get("tools")
        if isinstance(tools, list):
            return [str(t) for t in tools]
    return None


class TurnTelemetry:
    """Mutable accumulator for one agent turn's metrics."""

    __slots__ = ("final_text", "events", "error", "input_tokens", "output_tokens", "latency_s")

    def __init__(self) -> None:
        self.final_text = ""
        self.events = 0
        self.error: Optional[str] = None
        self.input_tokens = 0
        self.output_tokens = 0
        self.latency_s = 0.0


async def _collect_turn(
    config_yaml: str,
    prompt: str,
    workspace: Optional[str],
    tools: Optional[List[str]],
) -> TurnTelemetry:
    """Run one agent turn and return its telemetry (text, tokens, latency)."""
    text_parts: List[str] = []
    last_observation = ""
    telemetry = TurnTelemetry()
    start = time.monotonic()

    async for event in stream_exec(config_yaml, prompt, workspace, tools):
        telemetry.events += 1
        kind = event.get("type", "")
        if kind == "text_delta":
            text_parts.append(event.get("delta", ""))
        elif kind == "tool_end":
            last_observation = event.get("result", "")
        elif kind == "error":
            telemetry.error = event.get("message", "unknown error")
        elif kind in ("run_complete", "done"):
            telemetry.input_tokens += int(event.get("total_input_tokens", 0) or 0)
            telemetry.output_tokens += int(event.get("total_output_tokens", 0) or 0)

    telemetry.latency_s = time.monotonic() - start
    telemetry.final_text = "".join(text_parts).strip() or last_observation
    return telemetry


async def run_task(arm: Arm, task: Task, scorer: Scorer, trials: int, workspace: Optional[str]) -> TaskResult:
    """Run a single task with up to ``trials`` attempts (for pass^k)."""
    config_yaml = (WORKBENCH_ROOT / arm.config_path).read_text(encoding="utf-8")
    tools = _tools_from_config(config_yaml)

    successes = 0
    best_score = 0.0
    final_text = ""
    last_error: Optional[str] = None
    total_events = 0
    total_in = 0
    total_out = 0
    total_latency = 0.0

    for _ in range(max(1, trials)):
        turn = await _collect_turn(config_yaml, task.prompt, workspace, tools)
        total_events += turn.events
        total_in += turn.input_tokens
        total_out += turn.output_tokens
        total_latency += turn.latency_s
        final_text = turn.final_text or final_text
        last_error = turn.error or last_error
        ok, score = scorer(task, turn.final_text)
        best_score = max(best_score, score)
        if ok:
            successes += 1

    return TaskResult(
        task_id=task.task_id,
        arm=arm.name,
        final_text=final_text,
        success=successes > 0,
        score=best_score,
        trials=max(1, trials),
        events=total_events,
        error=last_error,
        input_tokens=total_in,
        output_tokens=total_out,
        latency_s=total_latency,
    )


async def run_arm(
    benchmark: str,
    arm: Arm,
    tasks: List[Task],
    scorer_name: str,
    trials: int = 1,
    workspace: Optional[str] = None,
) -> BenchmarkResult:
    """Run one arm over all tasks and aggregate metrics."""
    scorer = SCORERS[scorer_name]
    per_task: List[TaskResult] = []

    for task in tasks:
        result = await run_task(arm, task, scorer, trials, workspace)
        per_task.append(result)

    num = len(per_task) or 1
    accuracy = sum(1 for r in per_task if r.success) / num

    # pass^k: probability of succeeding on ALL k independent trials, estimated
    # per task as (successes/trials)^k averaged across tasks (tau-bench style).
    pass_hat_k = {}
    if trials > 1:
        for k in range(1, trials + 1):
            vals = []
            for r in per_task:
                p = (1.0 if r.success else 0.0)  # binary fallback when scorer is 0/1
                vals.append(p ** k)
            pass_hat_k[k] = sum(vals) / num

    return BenchmarkResult(
        benchmark=benchmark,
        arm=arm.name,
        num_tasks=len(per_task),
        accuracy=accuracy,
        pass_hat_k=pass_hat_k,
        per_task=per_task,
        total_input_tokens=sum(r.input_tokens for r in per_task),
        total_output_tokens=sum(r.output_tokens for r in per_task),
        total_latency_s=sum(r.latency_s for r in per_task),
    )


async def run_ablation(
    benchmark: str,
    arms: List[Arm],
    tasks: List[Task],
    scorer_name: str,
    trials: int = 1,
    workspace: Optional[str] = None,
) -> List[BenchmarkResult]:
    """Run every arm over the same task set — a controlled ablation."""
    results = []
    for arm in arms:
        results.append(await run_arm(benchmark, arm, tasks, scorer_name, trials, workspace))
    return results
