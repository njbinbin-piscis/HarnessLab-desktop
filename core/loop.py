"""Minimal Agent Loop implementation.

Follows the principle: "If there are tool calls, continue; otherwise, exit."
"""

from typing import Any, Dict, List, Optional
import logging

logger = logging.getLogger(__name__)


class AgentState:
    """Holds the current state of the agent execution."""

    def __init__(self):
        self.messages: List[Dict[str, Any]] = []
        self.is_finished: bool = False
        self.last_tool_calls: List[Dict[str, Any]] = []

    def add_message(self, role: str, content: Any):
        self.messages.append({"role": role, "content": content})

    def set_tool_calls(self, tool_calls: List[Dict[str, Any]]):
        self.last_tool_calls = tool_calls
        if not tool_calls:
            self.is_finished = True


class AgentLoop:
    """The core execution loop for the Agent Workbench."""

    def __init__(self, policy: Any, action_executor: Any):
        self.policy = policy
        self.action_executor = action_executor

    async def run(self, initial_state: Optional[AgentState] = None) -> AgentState:
        state = initial_state or AgentState()
        logger.info("Starting Agent Loop")

        while not state.is_finished:
            # 1. Policy decides the next action (usually calling LLM)
            logger.debug("Executing policy...")
            response = await self.policy.act(state)
            
            # 2. Update state with LLM response
            state.add_message("assistant", response.get("content"))
            
            # 3. Check for tool calls
            tool_calls = response.get("tool_calls", [])
            state.set_tool_calls(tool_calls)

            if state.is_finished:
                logger.info("No tool calls found. Exiting loop.")
                break

            # 4. Execute tools
            logger.debug(f"Executing {len(tool_calls)} tool calls...")
            for tool_call in tool_calls:
                result = await self.action_executor.execute(tool_call)
                state.add_message("tool", result)

        logger.info("Agent Loop finished")
        return state
