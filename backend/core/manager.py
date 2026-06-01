"""Experiment lifecycle management."""

import asyncio
import time
import uuid
from datetime import datetime
from typing import Dict, Optional, List

from backend.models.schemas import (
    ExperimentState,
    ExperimentConfig,
    AgentEvent,
    ValidationResult,
)
from backend.core.streaming import EventStream, create_event_stream


class ExperimentManager:
    """Manages the lifecycle of agent experiments.
    
    Handles experiment creation, execution tracking, and SSE streaming.
    """
    
    def __init__(self):
        self._experiments: Dict[str, ExperimentState] = {}
        self._event_streams: Dict[str, List[AgentEvent]] = {}
        self._stream_manager = create_event_stream()
    
    async def create_experiment(
        self, 
        config: ExperimentConfig,
        workspace_path: Optional[str] = None
    ) -> str:
        """Create a new experiment and return its ID.
        
        Args:
            config: Parsed experiment configuration
            workspace_path: Optional workspace directory path
            
        Returns:
            Unique experiment identifier
        """
        experiment_id = f"exp_{uuid.uuid4().hex[:8]}"
        
        state = ExperimentState(
            id=experiment_id,
            status="pending",
            config=config,
            start_time=datetime.utcnow(),
        )
        
        self._experiments[experiment_id] = state
        self._event_streams[experiment_id] = []
        
        # Register with stream manager
        self._stream_manager.register_experiment(experiment_id)
        
        return experiment_id
    
    async def start_experiment(self, experiment_id: str):
        """Mark an experiment as running.
        
        Args:
            experiment_id: The experiment to start
        """
        if experiment_id not in self._experiments:
            raise ValueError(f"Experiment {experiment_id} not found")
        
        self._experiments[experiment_id].status = "running"
        self._experiments[experiment_id].start_time = datetime.utcnow()
    
    async def push_event(self, experiment_id: str, event: AgentEvent):
        """Push an agent event to the experiment stream.
        
        Args:
            experiment_id: The target experiment
            event: The agent event to record
        """
        if experiment_id not in self._experiments:
            raise ValueError(f"Experiment {experiment_id} not found")
        
        # Store in history
        self._event_streams[experiment_id].append(event)
        
        # Push to SSE stream
        await self._stream_manager.push_event(experiment_id, event)
    
    async def complete_experiment(
        self, 
        experiment_id: str, 
        success: bool = True,
        error_message: Optional[str] = None
    ):
        """Mark an experiment as completed or failed.
        
        Args:
            experiment_id: The experiment to complete
            success: Whether the experiment succeeded
            error_message: Error message if failed
        """
        if experiment_id not in self._experiments:
            raise ValueError(f"Experiment {experiment_id} not found")
        
        state = self._experiments[experiment_id]
        state.status = "completed" if success else "failed"
        state.end_time = datetime.utcnow()
        
        if error_message:
            state.error_message = error_message
        
        # Calculate metrics
        if state.start_time and state.end_time:
            duration = (state.end_time - state.start_time).total_seconds()
            state.metrics["duration_seconds"] = duration
        
        # Count events by type
        events = self._event_streams.get(experiment_id, [])
        state.metrics["total_events"] = len(events)
        state.metrics["thought_count"] = sum(
            1 for e in events if e.type == "thought"
        )
        state.metrics["action_count"] = sum(
            1 for e in events if e.type == "action"
        )
        
        # Signal completion to stream
        await self._stream_manager.mark_complete(experiment_id)
    
    def get_experiment_state(self, experiment_id: str) -> Optional[ExperimentState]:
        """Get the current state of an experiment.
        
        Args:
            experiment_id: The experiment to query
            
        Returns:
            ExperimentState or None if not found
        """
        return self._experiments.get(experiment_id)
    
    def get_experiment_history(
        self, 
        experiment_id: str
    ) -> Optional[List[AgentEvent]]:
        """Get the full event history of an experiment.
        
        Args:
            experiment_id: The experiment to query
            
        Returns:
            List of AgentEvents or None if not found
        """
        return self._event_streams.get(experiment_id)
    
    def list_experiments(self) -> List[ExperimentState]:
        """List all experiments.
        
        Returns:
            List of all experiment states
        """
        return list(self._experiments.values())
    
    def cleanup_experiment(self, experiment_id: str):
        """Clean up resources for a completed experiment.
        
        Args:
            experiment_id: The experiment to clean up
        """
        self._stream_manager.cleanup(experiment_id)
        # Note: We keep the state and history for later retrieval
        # In production, you might want to persist to database


# Global experiment manager instance
_global_manager = ExperimentManager()


def get_experiment_manager() -> ExperimentManager:
    """Get the global experiment manager instance."""
    return _global_manager
