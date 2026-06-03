"""API tests for Phase 5 Layer B: /datasets and /eval-runs."""

import pytest
from fastapi.testclient import TestClient

from backend.main import app
from backend.core.store import get_run_store, TaskRecord


@pytest.fixture
def client():
    return TestClient(app)


def test_list_builtin_datasets(client):
    resp = client.get("/api/v1/datasets")
    assert resp.status_code == 200
    ids = {d["id"] for d in resp.json()["datasets"]}
    assert "smoke_gaia" in ids


def test_dataset_tasks(client):
    resp = client.get("/api/v1/datasets/smoke_gaia/tasks")
    assert resp.status_code == 200
    tasks = resp.json()["tasks"]
    assert tasks and "prompt" in tasks[0]


def test_dataset_404(client):
    assert client.get("/api/v1/datasets/nope").status_code == 404


def test_upload_validation_error(client):
    resp = client.post("/api/v1/datasets", json={"id": "bad_ds", "content": '{"no_prompt": 1}'})
    assert resp.status_code == 400


def test_upload_and_delete(client):
    body = {
        "id": "api_upload_ds",
        "content": '{"task_id": "x", "prompt": "hi", "answer": "ok"}',
        "scorer": "exact_match",
        "overwrite": True,
    }
    resp = client.post("/api/v1/datasets", json=body)
    assert resp.status_code == 200
    assert resp.json()["num_tasks"] == 1
    # cleanup
    assert client.delete("/api/v1/datasets/api_upload_ds").status_code == 200


def test_eval_run_read_endpoints(client):
    # Seed a run directly in the shared store, then read it back via the API.
    store = get_run_store()
    run_id = store.create_run(
        agent_name="Seeded", config_yaml="agent:\n  name: Seeded\n",
        dataset_id="smoke_gaia", scorer="exact_match", num_tasks=1, label="seed",
    )
    store.add_task_result(run_id, TaskRecord(task_id="t1", success=True, score=1.0))
    store.add_trajectory(run_id, "t1", [{"type": "text_delta", "delta": "hi"}])
    store.finalize_run(run_id, accuracy=1.0, total_input_tokens=1, total_output_tokens=1, total_latency_s=0.0)

    resp = client.get(f"/api/v1/eval-runs/{run_id}")
    assert resp.status_code == 200
    data = resp.json()
    assert data["run"]["accuracy"] == 1.0
    assert len(data["task_results"]) == 1

    traj = client.get(f"/api/v1/eval-runs/{run_id}/tasks/t1/trajectory")
    assert traj.status_code == 200 and traj.json()["events"]

    listing = client.get("/api/v1/eval-runs")
    assert listing.status_code == 200
    assert any(r["id"] == run_id for r in listing.json()["runs"])

    cmp = client.get("/api/v1/eval-runs/compare", params={"ids": run_id})
    assert cmp.status_code == 200
    assert cmp.json()["comparison"][0]["id"] == run_id


def test_eval_run_404(client):
    assert client.get("/api/v1/eval-runs/run_nope").status_code == 404


def test_start_eval_run_with_fake_stream(client, monkeypatch):
    # Patch the evaluator used by the endpoint so no binary/API key is needed.
    import backend.api.eval_runs as eval_runs

    async def fake_evaluate(*, out_queue=None, **kwargs):
        store = get_run_store()
        run_id = store.create_run(
            agent_name=kwargs.get("agent_name", "A"),
            config_yaml=kwargs["config_yaml"],
            dataset_id=kwargs["dataset_id"],
            scorer="exact_match", num_tasks=1,
        )
        store.finalize_run(run_id, accuracy=1.0, total_input_tokens=0, total_output_tokens=0, total_latency_s=0.0)
        if out_queue is not None:
            await out_queue.put({"type": "eval_start", "run_id": run_id, "num_tasks": 1})
            await out_queue.put({"type": "eval_complete", "run_id": run_id, "accuracy": 1.0})
        return {"type": "eval_complete", "run_id": run_id, "accuracy": 1.0}

    monkeypatch.setattr(eval_runs, "evaluate", fake_evaluate)

    resp = client.post("/api/v1/eval-runs", json={
        "config_yaml": "agent:\n  name: A\n",
        "dataset_id": "smoke_gaia",
    })
    assert resp.status_code == 200
    run_id = resp.json()["run_id"]
    assert run_id.startswith("run_")
    assert client.get(f"/api/v1/eval-runs/{run_id}").status_code == 200
