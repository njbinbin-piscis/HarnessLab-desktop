"""Tests for edge cases and boundary conditions in the Agent Loop."""

import pytest
from core.loop import AgentState, AgentLoop


class MockPolicy:
    def __init__(self, tool_calls_sequence):
        """
        :param tool_calls_sequence: A list of lists, where each inner list represents 
                                    the tool calls for that turn.
        """
        self.tool_calls_sequence = tool_calls_sequence
        self.turn_count = 0

    async def act(self, state):
        if self.turn_count >= len(self.tool_calls_sequence):
            return {"content": "Finished", "tool_calls": []}
        
        current_calls = self.tool_calls_sequence[self.turn_count]
        self.turn_count += 1
        return {"content": f"Turn {self.turn_count}", "tool_calls": current_calls}


class MockExecutor:
    def __init__(self, results=None):
        self.results = results or {}
        self.call_log = []

    async def execute(self, tool_call):
        self.call_log.append(tool_call)
        name = tool_call.get("name", "unknown")
        return self.results.get(name, f"Result of {name}")


@pytest.mark.asyncio
async def test_loop_with_multiple_tool_calls_in_one_turn():
    """Verify that the loop executes all tools provided in a single response."""
    state = AgentState()
    policy = MockPolicy(tool_calls_sequence=[
        [{"name": "tool_a"}, {"name": "tool_b"}],
        []
    ])
    executor = MockExecutor()
    loop = AgentLoop(policy=policy, action_executor=executor)
    
    final_state = await loop.run(state)
    
    assert final_state.is_finished is True
    assert len(executor.call_log) == 2
    assert executor.call_log[0]["name"] == "tool_a"
    assert executor.call_log[1]["name"] == "tool_b"


@pytest.mark.asyncio
async def test_loop_with_consecutive_turns_of_tool_calls():
    """Verify that the loop continues across multiple turns if tools are called."""
    state = AgentState()
    policy = MockPolicy(tool_calls_sequence=[
        [{"name": "step_1"}],
        [{"name": "step_2"}],
        []
    ])
    executor = MockExecutor()
    loop = AgentLoop(policy=policy, action_executor=executor)
    
    final_state = await loop.run(state)
    
    assert final_state.is_finished is True
    assert len(executor.call_log) == 2
    # Check message history structure
    # Turn 1: Assistant + Tool result
    # Turn 2: Assistant + Tool result
    # Turn 3: Assistant (final)
    assert len(final_state.messages) == 5 


@pytest.mark.asyncio
async def test_loop_handles_empty_initial_tool_calls():
    """Verify that the loop exits immediately if the first response has no tools."""
    state = AgentState()
    policy = MockPolicy(tool_calls_sequence=[[]])
    executor = MockExecutor()
    loop = AgentLoop(policy=policy, action_executor=executor)
    
    final_state = await loop.run(state)
    
    assert final_state.is_finished is True
    assert len(executor.call_log) == 0
    assert len(final_state.messages) == 1  # Only the initial assistant message


@pytest.mark.asyncio
async def test_loop_handles_exception_in_executor():
    """Verify behavior when a tool execution raises an exception."""
    class FailingExecutor:
        async def execute(self, tool_call):
            raise RuntimeError(f"Tool {tool_call['name']} failed!")

    state = AgentState()
    policy = MockPolicy(tool_calls_sequence=[[{"name": "failing_tool"}]])
    loop = AgentLoop(policy=policy, action_executor=FailingExecutor())
    
    with pytest.raises(RuntimeError):
        await loop.run(state)
