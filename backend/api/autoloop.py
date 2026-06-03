"""Self-improving design-loop endpoint (Pillar 2 + 3 closed loop).

POST starts the loop as a background task that persists each round's evaluation
run; progress streams over SSE. The final lineage (every candidate + its score)
and the best design are emitted as the terminal event and also recoverable from
the persisted runs.
"""

import asyncio
import json

from fastapi import APIRouter, HTTPException
from sse_starlette.sse import EventSourceResponse

from backend.core.autoloop import run_autoloop
from backend.core.datasets import DatasetError, get_dataset_registry
from backend.models.schemas import AutoLoopRequest

router = APIRouter(prefix="/autoloop", tags=["autoloop"])


@router.post("")
async def start_autoloop(request: AutoLoopRequest):
    """Run the propose→evaluate→revise loop and stream progress via SSE."""
    try:
        get_dataset_registry().get_spec(request.dataset_id)
    except DatasetError as exc:
        raise HTTPException(status_code=404, detail=str(exc))

    out_queue: asyncio.Queue = asyncio.Queue()

    async def runner():
        try:
            result = await run_autoloop(
                requirement=request.requirement,
                dataset_id=request.dataset_id,
                rounds=request.rounds,
                target_accuracy=request.target_accuracy,
                patience=request.patience,
                strategy_type=request.strategy_type,
                model=request.model,
                scorer_name=request.scorer,
                seeds=request.seeds,
                limit=request.limit,
                workspace=request.workspace_path,
                out_queue=out_queue,
            )
            await out_queue.put({"type": "autoloop_result", **result.to_dict()})
        except Exception as exc:  # noqa: BLE001
            await out_queue.put({"type": "error", "message": str(exc)})
            await out_queue.put({"type": "autoloop_complete", "error": True})

    async def event_gen():
        task = asyncio.create_task(runner())
        try:
            while True:
                event = await out_queue.get()
                yield {"data": json.dumps(event)}
                if event.get("type") in ("autoloop_result", "error"):
                    # autoloop_result is terminal; an error is followed by
                    # autoloop_complete which we also drain then stop.
                    if event.get("type") == "autoloop_result":
                        break
        finally:
            if not task.done():
                task.cancel()

    return EventSourceResponse(event_gen(), media_type="text/event-stream")
