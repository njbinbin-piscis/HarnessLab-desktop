"""Experiment management endpoints."""

import yaml
from fastapi import APIRouter, HTTPException
from sse_starlette.sse import EventSourceResponse

from backend.models.schemas import (
    ExperimentRequest,
    ExperimentResponse,
    AgentEvent,
)
from backend.core.manager import get_experiment_manager
from backend.core.runner import schedule_experiment
from backend.core.streaming import create_event_stream

router = APIRouter(prefix="/experiments", tags=["experiments"])

manager = get_experiment_manager()
stream_manager = create_event_stream()


@router.post("", response_model=ExperimentResponse)
async def start_experiment(request: ExperimentRequest):
    """Start a new agent experiment.
    
    Accepts YAML configuration and initiates experiment execution.
    Returns experiment ID for streaming and status tracking.
    """
    # Parse and validate YAML
    try:
        config_dict = yaml.safe_load(request.config_yaml)
        if config_dict is None:
            raise ValueError("Empty configuration")
    except yaml.YAMLError as e:
        raise HTTPException(status_code=400, detail=f"Invalid YAML: {str(e)}")
    
    # Create experiment
    from backend.models.schemas import ExperimentConfig
    try:
        config = ExperimentConfig(**config_dict)
    except Exception as e:
        raise HTTPException(status_code=400, detail=f"Invalid config schema: {str(e)}")
    
    experiment_id = await manager.create_experiment(
        config=config,
        workspace_path=request.workspace_path
    )
    
    await manager.start_experiment(experiment_id)

    schedule_experiment(
        manager,
        experiment_id,
        config,
        request.config_yaml,
        request.workspace_path,
        request.prompt,
    )

    return ExperimentResponse(
        experiment_id=experiment_id,
        status="running"
    )


@router.get("/{experiment_id}/stream")
async def stream_experiment(experiment_id: str):
    """Stream real-time agent execution events via SSE.
    
    Event types:
    - thought: Agent's internal reasoning
    - action: Tool call being prepared
    - observation: Tool execution result
    - error: Execution error
    - complete: Experiment finished
    """
    # Verify experiment exists
    state = manager.get_experiment_state(experiment_id)
    if not state:
        raise HTTPException(status_code=404, detail="Experiment not found")
    
    # Return SSE stream
    return EventSourceResponse(
        stream_manager.stream_events(experiment_id),
        media_type="text/event-stream"
    )


@router.get("/{experiment_id}/history")
async def get_experiment_history(experiment_id: str):
    """Retrieve full execution history for replay/analysis."""
    state = manager.get_experiment_state(experiment_id)
    if not state:
        raise HTTPException(status_code=404, detail="Experiment not found")
    
    history = manager.get_experiment_history(experiment_id)
    return {
        "experiment_id": experiment_id,
        "status": state.status,
        "events": history or []
    }


@router.get("/{experiment_id}/state")
async def get_experiment_state(experiment_id: str):
    """Get current experiment state and metrics."""
    state = manager.get_experiment_state(experiment_id)
    if not state:
        raise HTTPException(status_code=404, detail="Experiment not found")
    
    return state
