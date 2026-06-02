"""Tests for the core Agent Loop."""

import pytest
from core.loop import AgentState, AgentLoop


class MockPolicy:
    async def act(self, state):
        if len(state.messages) == 0:
            return {"content": "Thinking...", "tool_calls": [{"name": "test_tool"}]}
        return {"content": "Done", "tool_calls": []}


class MockExecutor:
    async def execute(self, tool_call):
        return f"Result of {tool_call['name']}"


@pytest.mark.asyncio
async def test_loop_exits_when_no_tool_calls():
    state = AgentState()
    loop = AgentLoop(policy=MockPolicy(), action_executor=MockExecutor())
    final_state = await loop.run(state)
    
    assert final_state.is_finished is True
    assert len(final_state.messages) == 3  # Assistant msg + tool result + final assistant msg
    assert final_state.messages[-1]["role"] == "assistant"


@pytest.mark.asyncio
async def test_loop_continues_with_tool_calls():
    state = AgentState()
    loop = AgentLoop(policy=MockPolicy(), action_executor=MockExecutor())
    final_state = await loop.run(state)
    
    # Verify the loop executed the tool and then exited on the second turn
    assert any(m["role"] == "tool" for m in final_state.messages)
