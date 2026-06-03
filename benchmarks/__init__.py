"""Paper-reproduction benchmark harness for Agent Workbench.

This package turns the reproduction YAML configs under ``configs/reproductions/``
into runnable, scorable experiments. Each benchmark is defined by:

* a **task loader** (yields ``Task`` records with a prompt and gold answer),
* a **scorer** (maps an agent transcript to a numeric score), and
* one or more **arms** (config YAMLs differing by a single controlled knob).

The harness reuses ``backend.core.engine.stream_exec`` so reproductions run
through the exact same Rust ``piscis-kernel`` path as the live platform — the
central validity claim of the platform paper.
"""

from benchmarks.types import Arm, BenchmarkResult, Task, TaskResult
from benchmarks.runner import run_arm, run_ablation

__all__ = [
    "Arm",
    "Task",
    "TaskResult",
    "BenchmarkResult",
    "run_arm",
    "run_ablation",
]
