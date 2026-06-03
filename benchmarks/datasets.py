"""Task loaders for reproduction benchmarks.

Real benchmark datasets (HotpotQA, GSM8K, GAIA, SWE-bench, tau-bench) are large
and gated behind their own licenses/downloads, so this module provides:

* ``load_jsonl`` — load a user-provided JSONL of tasks, and
* ``builtin_smoke_set`` — a tiny, dependency-free task set used for CI smoke
  tests and for validating the harness wiring end-to-end without downloads.

To run a real reproduction, point ``--tasks`` at a JSONL where each line has
``{"task_id", "prompt", "answer", "metadata"?}``.
"""

from __future__ import annotations

import json
from pathlib import Path
from typing import List

from benchmarks.types import Task


def load_jsonl(path: str) -> List[Task]:
    tasks: List[Task] = []
    with open(path, "r", encoding="utf-8") as handle:
        for line in handle:
            line = line.strip()
            if not line:
                continue
            row = json.loads(line)
            tasks.append(
                Task(
                    task_id=str(row.get("task_id") or row.get("id") or len(tasks)),
                    prompt=row["prompt"] if "prompt" in row else row["question"],
                    answer=row.get("answer"),
                    metadata=row.get("metadata", {}),
                )
            )
    return tasks


def builtin_smoke_set(benchmark: str) -> List[Task]:
    """A handful of dependency-free tasks per benchmark for smoke testing."""
    if benchmark in ("hotpotqa", "react"):
        return [
            Task("smoke-hp-1", "What is the capital of France?", "Paris"),
            Task(
                "smoke-hp-2",
                "Who wrote the play 'Romeo and Juliet'?",
                "William Shakespeare",
            ),
        ]
    if benchmark in ("gsm8k", "plan_and_solve"):
        return [
            Task(
                "smoke-gsm-1",
                "Natalia sold 48 clips in April and half as many in May. "
                "How many clips did she sell altogether? The answer is a number.",
                "72",
            ),
            Task(
                "smoke-gsm-2",
                "A robe takes 2 bolts of blue fiber and half that much white fiber. "
                "How many bolts in total? The answer is a number.",
                "3",
            ),
        ]
    if benchmark == "gaia":
        return [
            Task("smoke-gaia-1", "How many continents are there on Earth?", "7"),
        ]
    # Generic fallback.
    return [Task("smoke-1", "Reply with the single word: ok", "ok")]


def load_tasks(benchmark: str, path: str | None) -> List[Task]:
    if path and Path(path).exists():
        return load_jsonl(path)
    return builtin_smoke_set(benchmark)
