"""Evaluation-run endpoints: run an agent over a validation set, observe results.

POST starts a run as a background task (it persists to the store regardless of
whether anyone is watching). Live progress is observable over SSE; completed
runs and their per-task trajectories are queryable from the persistent store so
the platform stays *observable* after the fact and runs are comparable.
"""

import asyncio
import json
from typing import Dict, List, Optional, Tuple

from fastapi import APIRouter, HTTPException, Query
from sse_starlette.sse import EventSourceResponse

from backend.core.datasets import DatasetError, get_dataset_registry
from backend.core.evaluator import evaluate
from backend.core.store import get_run_store
from backend.models.schemas import EvalRunRequest

router = APIRouter(prefix="/eval-runs", tags=["eval-runs"])

# Live progress queues keyed by run_id, with a small replay buffer so a stream
# that attaches slightly after POST still sees the opening events.
_live: Dict[str, Tuple[asyncio.Queue, List[dict], asyncio.Task]] = {}

_TERMINAL = {"eval_complete", "error"}


@router.post("")
async def start_eval_run(request: EvalRunRequest):
    """Start an evaluation run; returns its persisted ``run_id`` immediately."""
    datasets = get_dataset_registry()
    try:
        datasets.get_spec(request.dataset_id)
    except DatasetError as exc:
        raise HTTPException(status_code=404, detail=str(exc))

    queue: asyncio.Queue = asyncio.Queue()
    buffer: List[dict] = []

    async def run():
        try:
            await evaluate(
                config_yaml=request.config_yaml,
                dataset_id=request.dataset_id,
                agent_name=request.agent_name,
                scorer_name=request.scorer,
                seeds=request.seeds,
                limit=request.limit,
                workspace=request.workspace_path,
                label=request.label,
                out_queue=queue,
            )
        except Exception as exc:  # noqa: BLE001
            await queue.put({"type": "error", "message": str(exc)})

    task = asyncio.create_task(run())

    # Wait for the opening event (carries the persisted run_id).
    first = await queue.get()
    buffer.append(first)
    if first.get("type") == "error":
        raise HTTPException(status_code=400, detail=first.get("message", "eval failed to start"))
    run_id = first["run_id"]
    _live[run_id] = (queue, buffer, task)
    return {"run_id": run_id, "status": "running"}


@router.get("")
async def list_eval_runs(limit: int = 100):
    """List recent evaluation runs (most recent first)."""
    return {"runs": get_run_store().list_runs(limit=limit)}


@router.get("/compare")
async def compare_eval_runs(ids: str = Query(..., description="Comma-separated run ids")):
    """Side-by-side aggregate metrics for several runs."""
    run_ids = [r.strip() for r in ids.split(",") if r.strip()]
    if not run_ids:
        raise HTTPException(status_code=400, detail="provide at least one run id")
    return {"comparison": get_run_store().compare(run_ids)}


@router.get("/{run_id}")
async def get_eval_run(run_id: str):
    """Fetch a run's metadata plus its per-task results."""
    store = get_run_store()
    run = store.get_run(run_id)
    if run is None:
        raise HTTPException(status_code=404, detail=f"run '{run_id}' not found")
    return {"run": run, "task_results": store.get_task_results(run_id)}


@router.get("/{run_id}/tasks/{task_id}/trajectory")
async def get_task_trajectory(run_id: str, task_id: str):
    """Fetch the ordered event trajectory for one task (replay)."""
    store = get_run_store()
    if store.get_run(run_id) is None:
        raise HTTPException(status_code=404, detail=f"run '{run_id}' not found")
    return {"run_id": run_id, "task_id": task_id, "events": store.get_trajectory(run_id, task_id)}


@router.get("/{run_id}/stream")
async def stream_eval_run(run_id: str):
    """Stream live progress for a running evaluation via SSE.

    If the run already finished, replays a terminal summary from the store so
    the consumer always terminates cleanly.
    """
    entry = _live.get(run_id)
    if entry is None:
        run = get_run_store().get_run(run_id)
        if run is None:
            raise HTTPException(status_code=404, detail=f"run '{run_id}' not found")

        async def replay():
            yield {"data": json.dumps({
                "type": "eval_complete",
                "run_id": run_id,
                "accuracy": run["accuracy"],
                "num_tasks": run["num_tasks"],
                "replay": True,
            })}

        return EventSourceResponse(replay(), media_type="text/event-stream")

    queue, buffer, _task = entry

    async def event_gen():
        for ev in list(buffer):
            yield {"data": json.dumps(ev)}
            if ev.get("type") in _TERMINAL:
                return
        try:
            while True:
                ev = await queue.get()
                buffer.append(ev)
                yield {"data": json.dumps(ev)}
                if ev.get("type") in _TERMINAL:
                    break
        finally:
            _live.pop(run_id, None)

    return EventSourceResponse(event_gen(), media_type="text/event-stream")
