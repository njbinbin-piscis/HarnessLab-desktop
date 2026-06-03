"""Shared dataclasses for the reproduction harness."""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any, Callable, Dict, List, Optional


@dataclass
class Task:
    """A single benchmark instance."""

    task_id: str
    prompt: str
    answer: Optional[str] = None
    metadata: Dict[str, Any] = field(default_factory=dict)


@dataclass
class TaskResult:
    """Outcome of running one task with one arm."""

    task_id: str
    arm: str
    final_text: str
    success: bool
    score: float
    trials: int = 1
    events: int = 0
    error: Optional[str] = None
    input_tokens: int = 0
    output_tokens: int = 0
    latency_s: float = 0.0


@dataclass
class Arm:
    """One experimental arm: a config YAML plus the knob it varies."""

    name: str
    config_path: str
    controlled_knob: str = ""
    description: str = ""


@dataclass
class BenchmarkResult:
    """Aggregated metrics for one arm over a task set."""

    benchmark: str
    arm: str
    num_tasks: int
    accuracy: float
    pass_hat_k: Dict[int, float] = field(default_factory=dict)
    per_task: List[TaskResult] = field(default_factory=list)
    total_input_tokens: int = 0
    total_output_tokens: int = 0
    total_latency_s: float = 0.0

    def summary(self) -> Dict[str, Any]:
        return {
            "benchmark": self.benchmark,
            "arm": self.arm,
            "num_tasks": self.num_tasks,
            "accuracy": round(self.accuracy, 4),
            "pass_hat_k": {k: round(v, 4) for k, v in self.pass_hat_k.items()},
            "total_input_tokens": self.total_input_tokens,
            "total_output_tokens": self.total_output_tokens,
            "total_latency_s": round(self.total_latency_s, 2),
        }


# A scorer maps (task, final_text) -> (success, score in [0,1]).
Scorer = Callable[[Task, str], "tuple[bool, float]"]
