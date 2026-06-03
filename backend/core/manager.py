"""Experiment lifecycle management."""

import asyncio
import time
import uuid
from datetime import datetime
from typing import Any, Dict, Optional, List

from backend.models.schemas import (
    ExperimentState,
    ExperimentConfig,
    AgentEvent,
    ValidationResult,
)
from backend.core.streaming import EventStream, create_event_stream
from backend.core.swarm import (
    Coordinator,
    CoordinationContext,
    MessageBus,
    extract_final,
)


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


class MultiAgentSession:
    """Concurrently manage N long-lived agent sessions and fan-in their events.

    Each value in ``sessions`` is an
    :class:`~backend.core.engine.InteractiveSession`-like object exposing
    ``start`` / ``events`` / ``send_prompt`` / ``inject`` / ``close``. A reader
    task per agent pumps that agent's NDJSON events into a per-agent queue so
    the orchestrator can drive turns one agent at a time while still surfacing
    a merged stream.
    """

    def __init__(self, sessions: Dict[str, Any]) -> None:
        self._sessions = sessions
        self._queues: Dict[str, asyncio.Queue] = {n: asyncio.Queue() for n in sessions}
        self._tasks: List[asyncio.Task] = []

    @property
    def names(self) -> List[str]:
        return list(self._sessions)

    async def start_all(self, ready_timeout: float = 10.0) -> None:
        for name, session in self._sessions.items():
            await session.start()
            self._tasks.append(asyncio.create_task(self._pump(name, session)))
        # Best-effort: consume the initial `session_ready` envelope per agent so
        # turn reads start from a clean queue.
        for name in self._sessions:
            try:
                while True:
                    ev = await asyncio.wait_for(self._queues[name].get(), ready_timeout)
                    if ev.get("type") in ("session_ready", "agent_closed", "error"):
                        break
            except asyncio.TimeoutError:
                pass

    async def _pump(self, name: str, session: Any) -> None:
        try:
            async for event in session.events():
                await self._queues[name].put(event)
        finally:
            await self._queues[name].put({"type": "agent_closed"})

    async def next_event(self, name: str) -> Dict[str, Any]:
        return await self._queues[name].get()

    async def send_prompt(self, name: str, text: str) -> None:
        await self._sessions[name].send_prompt(text)

    async def inject(self, name: str, text: str) -> None:
        await self._sessions[name].inject(text)

    async def close_all(self) -> None:
        for session in self._sessions.values():
            try:
                await session.close()
            except Exception:
                pass
        for task in self._tasks:
            task.cancel()


class SwarmOrchestrator:
    """Drive a multi-agent swarm using a pluggable :class:`Coordinator`.

    The orchestrator is the *Meta-Agent*: each round it reads the shared
    :class:`MessageBus` history, asks the coordinator who to activate next (and
    what to say), runs that agent's turn, and posts the agent's answer back to
    the bus — until the coordinator declares the swarm done or the round budget
    is exhausted.

    Events from each agent turn are tagged with the agent name and forwarded to
    ``out_queue`` (when provided) so an SSE endpoint can stream the whole swarm.
    """

    def __init__(
        self,
        multi: MultiAgentSession,
        coordinator: Coordinator,
        bus: MessageBus,
        task: str,
        max_rounds: int = 8,
        out_queue: Optional[asyncio.Queue] = None,
    ) -> None:
        self.multi = multi
        self.coordinator = coordinator
        self.bus = bus
        self.task = task
        self.max_rounds = max_rounds
        self.out_queue = out_queue

    async def _emit(self, event: Dict[str, Any]) -> None:
        if self.out_queue is not None:
            await self.out_queue.put(event)

    async def _run_agent_turn(self, name: str, message: str) -> str:
        """Send one prompt to ``name`` and collect its assistant text."""
        await self.multi.send_prompt(name, message)
        text_parts: List[str] = []
        while True:
            event = await self.multi.next_event(name)
            tagged = dict(event)
            tagged["agent"] = name
            await self._emit(tagged)
            etype = event.get("type")
            if etype == "text_delta":
                text_parts.append(event.get("delta", ""))
            elif etype in ("turn_complete", "agent_closed", "error"):
                break
        return "".join(text_parts).strip()

    async def run(self) -> Dict[str, Any]:
        """Run the coordination loop to completion. Returns a result summary."""
        await self._emit({"type": "swarm_start", "task": self.task, "agents": self.multi.names})

        final: Optional[str] = None
        rounds_run = 0
        for round_idx in range(self.max_rounds):
            rounds_run = round_idx + 1
            ctx = CoordinationContext(
                task=self.task,
                agents=self.multi.names,
                transcript=self.bus.history(),
                round=round_idx,
                max_rounds=self.max_rounds,
            )
            decision = self.coordinator.coordinate(ctx)
            await self._emit(
                {
                    "type": "coordination",
                    "round": round_idx,
                    "coordinator": getattr(self.coordinator, "name", "?"),
                    "next_agent": decision.next_agent,
                    "done": decision.done,
                    "metadata": decision.metadata,
                }
            )

            if decision.done:
                final = decision.final
                break

            if not decision.next_agent:
                # Coordinator yielded nothing actionable; stop to avoid spinning.
                break

            prompt = decision.message or self.task
            answer = await self._run_agent_turn(decision.next_agent, prompt)
            self.bus.post(decision.next_agent, answer)

            # Early exit if the agent itself declared a final answer.
            agent_final = extract_final(answer)
            if agent_final and getattr(self.coordinator, "name", "") == "blackboard":
                final = agent_final
                break

        result = {
            "type": "swarm_complete",
            "final": final,
            "rounds": rounds_run,
            "transcript": self.bus.history(),
        }
        await self._emit(result)
        return result
