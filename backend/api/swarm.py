"""Multi-agent swarm endpoint (Layer 3).

Starts N long-lived agent sessions, drives them with a pluggable coordinator
through a shared message bus, and streams the aggregated event flow over SSE.
"""

import asyncio
import json

from fastapi import APIRouter, HTTPException
from sse_starlette.sse import EventSourceResponse

from backend.core.engine import InteractiveSession, engine_available, missing_requirements
from backend.core.manager import MultiAgentSession, SwarmOrchestrator
from backend.core.swarm import MessageBus, builtin_coordinators, load_coordinator
from backend.models.schemas import SwarmRequest

router = APIRouter(prefix="/swarm", tags=["swarm"])


@router.get("/coordinators")
async def list_coordinators():
    """List the built-in coordination algorithms."""
    return {"builtin": builtin_coordinators()}


@router.post("")
async def start_swarm(request: SwarmRequest):
    """Start a multi-agent swarm and stream aggregated events via SSE.

    The request supplies the shared task, at least two agents (name +
    config YAML), and a coordinator (built-in name or user module). Events are
    tagged with the originating agent plus orchestration envelopes
    (``swarm_start`` / ``coordination`` / ``swarm_complete``).
    """
    if not engine_available():
        raise HTTPException(
            status_code=503,
            detail=missing_requirements() or "live engine unavailable",
        )

    names = [a.name for a in request.agents]
    if len(set(names)) != len(names):
        raise HTTPException(status_code=400, detail="agent names must be unique")

    try:
        coordinator = load_coordinator(request.coordinator)
    except Exception as e:  # noqa: BLE001 - surface config errors to the caller
        raise HTTPException(status_code=400, detail=f"invalid coordinator: {e}")

    sessions = {
        spec.name: InteractiveSession(spec.config_yaml, request.workspace_path)
        for spec in request.agents
    }
    multi = MultiAgentSession(sessions)
    bus = MessageBus()
    out_queue: asyncio.Queue = asyncio.Queue()
    orchestrator = SwarmOrchestrator(
        multi=multi,
        coordinator=coordinator,
        bus=bus,
        task=request.task,
        max_rounds=request.max_rounds,
        out_queue=out_queue,
    )

    async def runner():
        try:
            await multi.start_all()
            await orchestrator.run()
        except Exception as e:  # noqa: BLE001
            await out_queue.put({"type": "error", "message": f"swarm failed: {e}"})
            # Ensure the SSE consumer terminates.
            await out_queue.put({"type": "swarm_complete", "final": None, "error": True})
        finally:
            await multi.close_all()

    async def event_gen():
        task = asyncio.create_task(runner())
        try:
            while True:
                event = await out_queue.get()
                yield {"data": json.dumps(event)}
                if event.get("type") == "swarm_complete":
                    break
        finally:
            if not task.done():
                task.cancel()

    return EventSourceResponse(event_gen(), media_type="text/event-stream")
