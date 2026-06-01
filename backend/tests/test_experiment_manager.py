"""Tests for ExperimentManager and streaming."""

import pytest
import asyncio
from datetime import datetime

from backend.models.schemas import ExperimentConfig, AgentEvent
from backend.core.manager import ExperimentManager
from backend.core.streaming import EventStream


@pytest.fixture
def manager():
    """Create a fresh ExperimentManager instance."""
    return ExperimentManager()


@pytest.fixture
def sample_config():
    """Create a sample experiment configuration."""
    return ExperimentConfig(
        agent={
            "name": "TestAgent",
            "model": "gpt-4"
        }
    )


class TestExperimentManager:
    """Test suite for ExperimentManager."""
    
    @pytest.mark.asyncio
    async def test_create_experiment(self, manager, sample_config):
        """Test creating a new experiment."""
        experiment_id = await manager.create_experiment(sample_config)
        assert experiment_id.startswith("exp_")
        
        state = manager.get_experiment_state(experiment_id)
        assert state is not None
        assert state.status == "pending"
        assert state.config == sample_config
    
    @pytest.mark.asyncio
    async def test_start_experiment(self, manager, sample_config):
        """Test starting an experiment."""
        experiment_id = await manager.create_experiment(sample_config)
        await manager.start_experiment(experiment_id)
        
        state = manager.get_experiment_state(experiment_id)
        assert state.status == "running"
    
    @pytest.mark.asyncio
    async def test_push_event(self, manager, sample_config):
        """Test pushing events to an experiment."""
        experiment_id = await manager.create_experiment(sample_config)
        await manager.start_experiment(experiment_id)
        
        event = AgentEvent(
            timestamp=datetime.utcnow().timestamp(),
            type="thought",
            content="Let me think about this problem..."
        )
        
        await manager.push_event(experiment_id, event)
        
        history = manager.get_experiment_history(experiment_id)
        assert len(history) == 1
        assert history[0].type == "thought"
    
    @pytest.mark.asyncio
    async def test_complete_experiment_success(self, manager, sample_config):
        """Test completing an experiment successfully."""
        experiment_id = await manager.create_experiment(sample_config)
        await manager.start_experiment(experiment_id)
        
        # Add some events
        event = AgentEvent(
            timestamp=datetime.utcnow().timestamp(),
            type="action",
            content="Calling file_read"
        )
        await manager.push_event(experiment_id, event)
        
        await manager.complete_experiment(experiment_id, success=True)
        
        state = manager.get_experiment_state(experiment_id)
        assert state.status == "completed"
        assert state.metrics["total_events"] == 1
        assert state.metrics["action_count"] == 1
    
    @pytest.mark.asyncio
    async def test_complete_experiment_failure(self, manager, sample_config):
        """Test completing an experiment with failure."""
        experiment_id = await manager.create_experiment(sample_config)
        await manager.start_experiment(experiment_id)
        
        error_msg = "Connection timeout"
        await manager.complete_experiment(
            experiment_id, 
            success=False, 
            error_message=error_msg
        )
        
        state = manager.get_experiment_state(experiment_id)
        assert state.status == "failed"
        assert state.error_message == error_msg
    
    @pytest.mark.asyncio
    async def test_get_nonexistent_experiment(self, manager):
        """Test getting state of nonexistent experiment."""
        state = manager.get_experiment_state("exp_nonexistent")
        assert state is None
    
    @pytest.mark.asyncio
    async def test_list_experiments(self, manager, sample_config):
        """Test listing all experiments."""
        await manager.create_experiment(sample_config)
        await manager.create_experiment(sample_config)
        
        experiments = manager.list_experiments()
        assert len(experiments) == 2
    
    @pytest.mark.asyncio
    async def test_invalid_experiment_operations(self, manager):
        """Test operations on invalid experiment ID."""
        with pytest.raises(ValueError):
            await manager.start_experiment("exp_invalid")
        
        with pytest.raises(ValueError):
            event = AgentEvent(
                timestamp=datetime.utcnow().timestamp(),
                type="thought",
                content="test"
            )
            await manager.push_event("exp_invalid", event)


class TestEventStream:
    """Test suite for EventStream."""
    
    @pytest.mark.asyncio
    async def test_register_and_stream(self):
        """Test registering experiment and streaming events."""
        stream = EventStream()
        experiment_id = "exp_test"
        
        queue = stream.register_experiment(experiment_id)
        
        # Push an event
        event = AgentEvent(
            timestamp=datetime.utcnow().timestamp(),
            type="thought",
            content="Testing stream"
        )
        await stream.push_event(experiment_id, event)
        
        # Mark complete
        await stream.mark_complete(experiment_id)
        
        # Consume stream
        events_received = []
        async for sse_event in stream.stream_events(experiment_id):
            events_received.append(sse_event)
        
        assert len(events_received) == 2  # One thought + one complete sentinel
        assert events_received[0]["event"] == "thought"
        assert events_received[1]["event"] == "complete"
    
    @pytest.mark.asyncio
    async def test_cleanup(self):
        """Test cleaning up completed experiment."""
        stream = EventStream()
        experiment_id = "exp_test"
        
        stream.register_experiment(experiment_id)
        stream.cleanup(experiment_id)
        
        # Should raise error when trying to stream after cleanup
        with pytest.raises(ValueError):
            async for _ in stream.stream_events(experiment_id):
                pass
