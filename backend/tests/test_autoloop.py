"""Tests for the self-improving design loop (Phase 5 closed loop)."""

import asyncio

import pytest

from backend.core.autoloop import LineageEntry, run_autoloop
from backend.core.datasets import DatasetRegistry
from backend.core.store import RunStore


def _fixture_env(tmp_path):
    datasets = DatasetRegistry(data_dir=tmp_path)
    store = RunStore(db_path=tmp_path / "runs.db")
    return datasets, store


@pytest.mark.asyncio
async def test_autoloop_stops_on_target(tmp_path):
    datasets, store = _fixture_env(tmp_path)

    async def proposer(round_idx, requirement, history):
        return f"agent:\n  name: cand_{round_idx}\n"

    # Round 0 scores 1.0 → target reached immediately.
    async def evaluator_fn(*, config_yaml, dataset_id, lineage=None, **kwargs):
        run_id = store.create_run(
            agent_name=kwargs.get("agent_name", "a"), config_yaml=config_yaml,
            dataset_id=dataset_id, scorer="exact_match", num_tasks=1, lineage=lineage,
        )
        store.finalize_run(run_id, accuracy=1.0, total_input_tokens=0, total_output_tokens=0, total_latency_s=0.0)
        return {"run_id": run_id, "accuracy": 1.0}

    result = await run_autoloop(
        requirement="x", dataset_id="smoke_gaia", rounds=5,
        proposer=proposer, evaluator_fn=evaluator_fn, store=store, datasets=datasets,
    )
    assert result.stopped_reason == "target_reached"
    assert result.rounds_run == 1
    assert result.best_accuracy == 1.0
    assert result.best_run_id is not None


@pytest.mark.asyncio
async def test_autoloop_improves_then_stops_on_patience(tmp_path):
    datasets, store = _fixture_env(tmp_path)
    scores = [0.2, 0.6, 0.6, 0.6]  # improves once, then plateaus

    async def proposer(round_idx, requirement, history):
        return f"agent:\n  name: cand_{round_idx}\n"

    async def evaluator_fn(*, config_yaml, dataset_id, lineage=None, **kwargs):
        rnd = lineage["round"]
        run_id = store.create_run(
            agent_name="a", config_yaml=config_yaml, dataset_id=dataset_id,
            scorer="exact_match", num_tasks=1, lineage=lineage,
        )
        acc = scores[rnd]
        store.finalize_run(run_id, accuracy=acc, total_input_tokens=0, total_output_tokens=0, total_latency_s=0.0)
        return {"run_id": run_id, "accuracy": acc}

    result = await run_autoloop(
        requirement="x", dataset_id="smoke_gaia", rounds=10, target_accuracy=0.99,
        patience=2, proposer=proposer, evaluator_fn=evaluator_fn, store=store, datasets=datasets,
    )
    # best is round 1 (0.6); rounds 2 & 3 fail to improve → patience(2) triggers
    assert result.best_round == 1
    assert result.best_accuracy == pytest.approx(0.6)
    assert result.stopped_reason == "no_improvement"
    # lineage links each round to its parent run.
    assert result.lineage[1].parent_run_id == result.lineage[0].run_id


@pytest.mark.asyncio
async def test_autoloop_emits_progress(tmp_path):
    datasets, store = _fixture_env(tmp_path)
    out: asyncio.Queue = asyncio.Queue()

    async def proposer(round_idx, requirement, history):
        return "agent:\n  name: c\n"

    async def evaluator_fn(*, config_yaml, dataset_id, lineage=None, **kwargs):
        run_id = store.create_run(
            agent_name="a", config_yaml=config_yaml, dataset_id=dataset_id,
            scorer="exact_match", num_tasks=1, lineage=lineage,
        )
        store.finalize_run(run_id, accuracy=1.0, total_input_tokens=0, total_output_tokens=0, total_latency_s=0.0)
        return {"run_id": run_id, "accuracy": 1.0}

    await run_autoloop(
        requirement="x", dataset_id="smoke_gaia", rounds=1,
        proposer=proposer, evaluator_fn=evaluator_fn, out_queue=out, store=store, datasets=datasets,
    )
    types = []
    while not out.empty():
        types.append(out.get_nowait()["type"])
    assert "autoloop_start" in types
    assert "round_evaluated" in types
    assert "autoloop_complete" in types
