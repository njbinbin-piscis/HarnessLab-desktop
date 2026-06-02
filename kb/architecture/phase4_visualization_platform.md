# Phase 4: Visualization & Experiment Platform Architecture

## 1. Overview
Phase 4 aims to transform the Agent Workbench from a CLI-based engine into an interactive, web-based experiment platform. This allows AI scientists to visually design agents, monitor execution in real-time, and analyze performance without touching code.

## 2. Technical Stack Selection

### 2.1 Backend: Python FastAPI
- **Rationale**: 
  - High performance with async support for streaming LLM responses.
  - Native integration with `pydantic` for robust YAML/JSON validation.
  - Easy integration with existing Python-based testing and benchmarking tools.
  - `Server-Sent Events (SSE)` support for real-time state streaming.
- **Key Libraries**:
  - `fastapi`: Web framework.
  - `uvicorn`: ASGI server.
  - `pyyaml`: YAML parsing.
  - `sse-starlette`: SSE implementation.
  - `pisci-kernel` (via PyO3 or subprocess): Core engine interaction.

### 2.2 Frontend: React + TypeScript
- **Rationale**:
  - Component-based architecture fits the modular nature of agent visualization.
  - Rich ecosystem for code editors (`monaco-editor`) and graph visualization (`react-flow`).
  - Strong typing ensures consistency with backend API contracts.
- **Key Libraries**:
  - `vite`: Build tool.
  - `react-flow`: For execution path visualization.
  - `monaco-editor`: For YAML configuration editing.
  - `tailwindcss`: UI styling.
  - `zustand`: State management.

## 3. Backend API Interface Specification

### 3.1 Core Endpoints

#### `POST /api/v1/experiments`
- **Description**: Start a new agent experiment.
- **Request**:
  ```json
  {
    "config_yaml": "agent:\n  name: Test\n...",
    "workspace_path": "/path/to/workspace"
  }
  ```
- **Response**:
  ```json
  {
    "experiment_id": "exp_12345",
    "status": "running"
  }
  ```

#### `GET /api/v1/experiments/{id}/stream`
- **Description**: Stream real-time agent execution state.
- **Protocol**: Server-Sent Events (SSE).
- **Event Types**:
  - `thought`: Agent's internal reasoning.
  - `action`: Tool call being prepared.
  - `observation`: Tool execution result.
  - `error`: Execution error.
  - `complete`: Experiment finished.
- **Event Data Format**:
  ```json
  {
    "timestamp": 1717234567890,
    "type": "action",
    "data": {
      "tool_name": "file_read",
      "arguments": { "path": "README.md" }
    }
  }
  ```

#### `GET /api/v1/experiments/{id}/history`
- **Description**: Retrieve full execution history for replay/analysis.
- **Response**: Array of state snapshots.

#### `POST /api/v1/config/validate`
- **Description**: Validate YAML configuration without running.
- **Request**: YAML string.
- **Response**: Validation errors or success.

### 3.2 Data Models

```python
class ExperimentState(BaseModel):
    id: str
    status: Literal["pending", "running", "completed", "failed"]
    config: Dict[str, Any]
    start_time: datetime
    end_time: Optional[datetime]
    metrics: Dict[str, float]  # token_usage, duration, etc.

class AgentEvent(BaseModel):
    type: Literal["thought", "action", "observation", "error"]
    content: str
    metadata: Optional[Dict[str, Any]]
```

## 4. Frontend Component Structure

### 4.1 Layout
- **Sidebar**: Experiment list, config templates.
- **Main Area**: 
  - **Top**: YAML Editor (Monaco) with live validation.
  - **Middle**: Real-time Log Stream (Thoughts/Actions/Observations).
  - **Bottom**: Execution Path Visualization (React Flow).

### 4.2 Key Components

#### `ConfigEditor`
- Integrates `monaco-editor` with YAML language support.
- Real-time validation via `/api/v1/config/validate`.
- "Run Experiment" button triggers `POST /api/v1/experiments`.

#### `LiveLogStream`
- Subscribes to SSE endpoint.
- Renders events in a collapsible, color-coded list.
- Supports filtering by event type (e.g., show only `action`).

#### `ExecutionGraph`
- Uses `react-flow` to visualize the agent's decision tree.
- Nodes: Thoughts, Actions, Observations.
- Edges: Causal relationships between steps.
- Interactive: Click node to see detailed content.

#### `ExperimentDashboard`
- Summary metrics: Total tokens, duration, tool call count.
- Performance charts: Token usage per turn, latency heatmap.

## 5. Implementation Plan

### Phase 4.1: Backend Foundation (Week 1)
- [ ] Initialize FastAPI project structure.
- [ ] Implement `/api/v1/config/validate` endpoint.
- [ ] Create `ExperimentManager` to handle lifecycle.
- [ ] Integrate with `pisci-engine` (subprocess or PyO3 binding).

### Phase 4.2: Streaming & State Management (Week 2)
- [ ] Implement SSE streaming for agent events.
- [ ] Define `AgentEvent` schema and serialization.
- [ ] Add in-memory storage for experiment history.

### Phase 4.3: Frontend Scaffold (Week 3)
- [ ] Initialize React + Vite project.
- [ ] Implement `ConfigEditor` with Monaco.
- [ ] Create basic layout and API client.

### Phase 4.4: Visualization & Integration (Week 4)
- [ ] Implement `LiveLogStream` component.
- [ ] Integrate `react-flow` for `ExecutionGraph`.
- [ ] Connect frontend to backend SSE stream.
- [ ] Add error handling and loading states.

### Phase 4.5: Polish & Testing (Week 5)
- [ ] Add experiment history/replay feature.
- [ ] Implement performance dashboard charts.
- [ ] End-to-end testing of common workflows.

## 6. Success Metrics
- [ ] User can edit YAML and see real-time validation errors.
- [ ] User can start an experiment and see thoughts/actions stream in < 1s latency.
- [ ] Execution graph correctly represents the agent's decision path.
- [ ] Platform supports at least 5 concurrent experiments.

[ProjectStatus] follow_up_needed
@!Coder Please begin implementing Phase 4.1: Backend Foundation. 
1. Initialize a `backend/` directory with FastAPI.
2. Implement the `/api/v1/config/validate` endpoint using the existing `src/config.rs` logic (exposed via PyO3 or a simple wrapper).
3. Create the `ExperimentManager` skeleton.
Deliverables:
- `backend/main.py`
- `backend/api/config.py`
- `backend/core/manager.py`
