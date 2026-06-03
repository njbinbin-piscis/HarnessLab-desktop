"""Tests for Phase 5 Layer B: datasets, run store, and the evaluator."""

import asyncio
from typing import Any, Dict, List, Optional

import pytest

from backend.core.datasets import DatasetError, DatasetRegistry
from backend.core.evaluator import evaluate
from backend.core.store import RunStore, TaskRecord


# --------------------------------------------------------------------------- #
# DatasetRegistry
# --------------------------------------------------------------------------- #


def test_builtin_datasets_present(tmp_path):
    reg = DatasetRegistry(data_dir=tmp_path)
    ids = {s.id for s in reg.list()}
    assert {"smoke_hotpotqa", "smoke_gsm8k", "smoke_gaia"} <= ids
    spec = reg.get_spec("smoke_gsm8k")
    assert spec.scorer == "numeric_match"
    assert spec.num_tasks >= 1


def test_register_and_load_jsonl(tmp_path):
    reg = DatasetRegistry(data_dir=tmp_path)
    content = (
        '{"task_id": "a", "prompt": "2+2? answer with a number", "answer": "4"}\n'
        '{"task_id": "b", "prompt": "3+3? answer with a number", "answer": "6"}\n'
    )
    spec = reg.register_jsonl("mymath", content, name="My Math", scorer="numeric_match")
    assert spec.num_tasks == 2 and spec.scorer == "numeric_match"
    tasks = reg.load("mymath")
    assert len(tasks) == 2 and tasks[0].answer == "4"
    # round-trips through a fresh registry over the same dir
    reg2 = DatasetRegistry(data_dir=tmp_path)
    assert "mymath" in {s.id for s in reg2.list()}


def test_invalid_jsonl_rejected(tmp_path):
    reg = DatasetRegistry(data_dir=tmp_path)
    with pytest.raises(DatasetError):
        reg.register_jsonl("bad", '{"no_prompt": 1}\n')
    with pytest.raises(DatasetError):
        reg.register_jsonl("empty", "\n\n")


def test_cannot_clobber_builtin(tmp_path):
    reg = DatasetRegistry(data_dir=tmp_path)
    with pytest.raises(DatasetError):
        reg.register_jsonl("smoke_gaia", '{"prompt": "x", "answer": "y"}\n')


# --------------------------------------------------------------------------- #
# RunStore
# --------------------------------------------------------------------------- #


def test_run_store_lifecycle(tmp_path):
    store = RunStore(db_path=tmp_path / "runs.db")
    run_id = store.create_run(
        agent_name="A", config_yaml="agent:\n  name: A\n",
        dataset_id="smoke_gaia", scorer="exact_match", num_tasks=1, label="baseline",
    )
    store.set_status(run_id, "running")
    store.add_task_result(run_id, TaskRecord(task_id="t1", success=True, score=1.0, input_tokens=10))
    store.add_trajectory(run_id, "t1", [{"type": "text_delta", "delta": "hello"}])
    store.finalize_run(run_id, accuracy=1.0, total_input_tokens=10, total_output_tokens=5, total_latency_s=0.1)

    run = store.get_run(run_id)
    assert run["status"] == "completed" and run["accuracy"] == 1.0
    assert run["completed_tasks"] == 1
    results = store.get_task_results(run_id)
    assert len(results) == 1 and results[0]["success"] == 1
    traj = store.get_trajectory(run_id, "t1")
    assert traj and traj[0]["type"] == "text_delta"
    comparison = store.compare([run_id])
    assert comparison[0]["accuracy"] == 1.0


# --------------------------------------------------------------------------- #
# Evaluator (with a fake agent stream — no binary/API key needed)
# --------------------------------------------------------------------------- #


def _fake_stream(answer_by_prompt):
    async def stream_fn(config_yaml: str, prompt: str, workspace, tools):
        ans = answer_by_prompt(prompt)
        yield {"type": "run_start"}
        yield {"type": "text_delta", "delta": f"FINAL ANSWER: {ans}"}
        yield {"type": "run_complete", "total_input_tokens": 7, "total_output_tokens": 3}

    return stream_fn


@pytest.mark.asyncio
async def test_evaluate_scores_and_persists(tmp_path):
    datasets = DatasetRegistry(data_dir=tmp_path)
    datasets.register_jsonl(
        "qa",
        '{"task_id": "q1", "prompt": "capital of France?", "answer": "Paris"}\n'
        '{"task_id": "q2", "prompt": "capital of Japan?", "answer": "Tokyo"}\n',
        scorer="exact_match",
    )
    store = RunStore(db_path=tmp_path / "runs.db")

    def answer(prompt: str) -> str:
        return "Paris" if "France" in prompt else "WRONG"

    out: asyncio.Queue = asyncio.Queue()
    summary = await evaluate(
        config_yaml="agent:\n  name: A\n",
        dataset_id="qa",
        scorer_name="exact_match",
        out_queue=out,
        stream_fn=_fake_stream(answer),
        store=store,
        datasets=datasets,
    )
    # One of two correct → accuracy 0.5
    assert summary["accuracy"] == 0.5
    assert summary["total_input_tokens"] == 14  # 7 per task * 2

    run = store.get_run(summary["run_id"])
    assert run["status"] == "completed"
    results = {r["task_id"]: r for r in store.get_task_results(summary["run_id"])}
    assert results["q1"]["success"] == 1
    assert results["q2"]["success"] == 0
    # trajectory captured for replay
    assert store.get_trajectory(summary["run_id"], "q1")


@pytest.mark.asyncio
async def test_evaluate_unknown_scorer(tmp_path):
    datasets = DatasetRegistry(data_dir=tmp_path)
    store = RunStore(db_path=tmp_path / "runs.db")
    with pytest.raises(ValueError):
        await evaluate(
            config_yaml="agent:\n  name: A\n",
            dataset_id="smoke_gaia",
            scorer_name="does_not_exist",
            stream_fn=_fake_stream(lambda p: "x"),
            store=store,
            datasets=datasets,
        )
