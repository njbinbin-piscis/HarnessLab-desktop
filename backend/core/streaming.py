"""SSE streaming support for agent events."""

import asyncio
import json
import time
from typing import AsyncGenerator, Dict, Any
from sse_starlette.sse import EventSourceResponse

from backend.models.schemas import AgentEvent


class EventStream:
    """Manages SSE event streams for experiment execution."""
    
    def __init__(self):
        self._queues: Dict[str, asyncio.Queue] = {}
        self._completed: set = set()
    
    def register_experiment(self, experiment_id: str) -> asyncio.Queue:
        """Register a new experiment stream and return its queue."""
        queue = asyncio.Queue()
        self._queues[experiment_id] = queue
        return queue
    
    async def push_event(self, experiment_id: str, event: AgentEvent):
        """Push an event to the experiment's stream."""
        if experiment_id in self._queues:
            await self._queues[experiment_id].put(event)
    
    async def mark_complete(self, experiment_id: str):
        """Mark an experiment as complete (sends sentinel None)."""
        if experiment_id in self._queues:
            await self._queues[experiment_id].put(None)
            self._completed.add(experiment_id)
    
    async def stream_events(self, experiment_id: str) -> AsyncGenerator[dict, None]:
        """Yield events as SSE-compatible dictionaries."""
        if experiment_id not in self._queues:
            raise ValueError(f"No stream registered for experiment {experiment_id}")
        
        queue = self._queues[experiment_id]
        
        while True:
            event = await queue.get()
            
            # Sentinel value indicates stream completion
            if event is None:
                yield {
                    "event": "complete",
                    "data": json.dumps({"status": "completed"})
                }
                break
            
            # Convert AgentEvent to SSE format
            event_type = event.type
            event_data = {
                "timestamp": event.timestamp,
                "type": event.type,
                "content": event.content,
                "metadata": event.metadata
            }
            
            yield {
                "event": event_type,
                "data": json.dumps(event_data)
            }
    
    def cleanup(self, experiment_id: str):
        """Remove completed experiment stream."""
        if experiment_id in self._queues:
            del self._queues[experiment_id]
        self._completed.discard(experiment_id)


# Global event stream manager
_global_stream = EventStream()


def create_event_stream() -> EventStream:
    """Get or create the global event stream manager."""
    return _global_stream
