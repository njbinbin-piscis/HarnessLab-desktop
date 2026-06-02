# Agent Workbench Benchmark Test Framework

This framework validates the configuration-driven architecture of the Agent Workbench. It ensures that switching YAML configurations correctly changes the Agent's behavior without code modifications.

## 1. Test Cases

### 1.1 ReAct Strategy Test
- **Configuration**: `configs/react_agent.yaml`
- **Expected Behavior**: The agent should interleave `Thought`, `Action`, and `Observation` steps. It should use tools frequently to gather information before providing a final answer.
- **Verification**: Check the conversation history for the presence of "Thought:" and "Action:" patterns in the LLM's responses.

### 1.2 Plan-and-Solve Strategy Test
- **Configuration**: `configs/plan_and_solve_agent.yaml`
- **Expected Behavior**: The agent should first produce a structured plan (numbered list) and then execute it step-by-step.
- **Verification**: Check the initial response for a "Plan" or numbered list. Ensure subsequent actions align with the planned steps.

## 2. Validation Logic

The validation is performed by the `BenchmarkRunner` which:
1. Loads the YAML configuration.
2. Instantiates the `HarnessConfig` using the `ConfigLoader`.
3. Runs a standardized test prompt (e.g., "Research the latest trends in AI agents and summarize them").
4. Analyzes the resulting message history against the expected strategy patterns.

## 3. Performance Monitoring (Timing Hook)

A `TimingHook` is implemented to record execution metrics at key lifecycle events:
- `before_tool`: Records the start time of a tool execution.
- `after_tool`: Records the end time and calculates duration.
- `on_llm_call_event`: Records the latency of LLM requests.

These metrics are stored in a `BenchmarkResult` struct and can be exported as JSON for analysis.

## 4. Implementation Details

### 4.1 Config Loader
Located in `src/config/loader.rs`, it maps YAML fields to `HarnessConfig` builder calls.

### 4.2 Strategy Validator
Located in `src/bench/validator.rs`, it uses simple pattern matching on the `LlmMessage` content to identify the active strategy.

### 4.3 Timing Hook
Located in `src/hooks/timing.rs`, it implements the `AgentHooks` trait to capture timestamps.
