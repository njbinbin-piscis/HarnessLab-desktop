# Phase 4 Extension: Meta-Agent Module Architecture

## 1. Overview
The **Meta-Agent** is a specialized, pre-configured Agent within the Workbench designed to automate the creation of other Agents. It acts as an "Agent Factory," translating high-level natural language requirements from researchers into executable YAML configurations, Prompt templates, and experimental workflows.

## 2. Functional Definition
- **Input**: Natural language description of the desired agent (e.g., "Create a Python code reviewer that checks for PEP8 compliance and suggests optimizations").
- **Output**: 
  - `agent_config.yaml`: Complete configuration file including model settings, plugins, and policies.
  - `system_prompt.md`: Optimized system prompt tailored to the agent's role.
  - `workflow.json` (Optional): A structured plan or tool-chain definition for complex agents.
- **Core Capability**: Leverages the existing `pisci-engine` kernel with a specialized "Architect" persona and access to a library of agent design patterns.

## 3. Backend API Interface Specification

### 3.1 Endpoint: `POST /api/v1/meta-agent/generate`
- **Description**: Generate an agent configuration draft based on a natural language description.
- **Request Body**:
  ```json
  {
    "description": "A data analysis agent that uses pandas to process CSV files and generates matplotlib charts.",
    "constraints": {
      "model_preference": "qwen-plus",
      "tools_allowed": ["file_read", "code_run", "file_write"],
      "max_turns": 10
    },
    "template_style": "react" // or "plan-and-solve"
  }
  ```
- **Response**:
  ```json
  {
    "status": "success",
    "draft_id": "draft_7890",
    "config_yaml": "agent:\n  name: DataAnalyst\n...",
    "system_prompt": "You are a data analysis expert...",
    "suggested_tools": ["file_read", "code_run"],
    "confidence_score": 0.92
  }
  ```

### 3.2 Endpoint: `POST /api/v1/meta-agent/refine`
- **Description**: Refine an existing draft based on user feedback or additional constraints.
- **Request Body**:
  ```json
  {
    "draft_id": "draft_7890",
    "feedback": "The agent should also be able to connect to SQL databases.",
    "updated_constraints": {
      "tools_allowed": ["file_read", "code_run", "file_write", "sql_query"]
    }
  }
  ```
- **Response**: Updated `config_yaml` and `system_prompt`.

### 3.3 Endpoint: `GET /api/v1/meta-agent/templates`
- **Description**: Retrieve a list of available agent design patterns/templates.
- **Response**:
  ```json
  [
    { "id": "react_basic", "name": "Basic ReAct Agent", "description": "..." },
    { "id": "coder_expert", "name": "Python Coding Expert", "description": "..." }
  ]
  ```

## 4. Internal Architecture & Logic

### 4.1 The Meta-Agent Configuration (`meta_agent.yaml`)
The Meta-Agent itself is defined by a YAML config stored in `configs/meta_agent.yaml`.
- **Persona**: "Senior AI Architect"
- **Tools**: 
  - `file_read`: To read existing templates and best practices from `kb/`.
  - `file_write`: To save generated drafts to a temporary staging area.
  - `yaml_validate`: A custom tool to ensure the generated YAML is syntactically correct.
- **Prompt Strategy**: 
  - Uses a "Chain-of-Thought" approach to first analyze the requirements, then select appropriate tools/plugins, and finally generate the config.

### 4.2 Generation Pipeline
1. **Requirement Analysis**: The Meta-Agent parses the user's description to identify key roles, required tools, and constraints.
2. **Pattern Matching**: It searches `kb/architecture/patterns/` for similar agent designs.
3. **Drafting**: 
   - Generates `system_prompt` using a dedicated LLM call.
   - Constructs `agent_config.yaml` by filling in model, plugins, and policy fields.
4. **Validation**: Runs the generated YAML through the `/api/v1/config/validate` endpoint internally.
5. **Refinement Loop**: If validation fails, it self-corrects up to 3 times before returning the result with error notes.

## 5. Frontend Interaction Flow: "Agent Generation Wizard"

### 5.1 Step 1: Description Input
- A large text area for the user to describe the agent.
- Optional sliders for "Creativity" (Temperature) and "Strictness" (Policy).

### 5.2 Step 2: Live Preview & Editing
- **Split View**: 
  - Left: Generated YAML config (editable via Monaco Editor).
  - Right: Generated System Prompt (editable).
- **Validation Status**: Real-time indicator showing if the config is valid.

### 5.3 Step 3: Test Drive
- A "Run Sandbox" button that launches a short, isolated experiment using the generated config.
- Results are displayed in the standard `LiveLogStream` component.

### 5.4 Step 4: Save & Deploy
- "Save to Library": Stores the config in `configs/custom/`.
- "Start Experiment": Launches a full-scale experiment with this new agent.

## 6. Implementation Plan for Meta-Agent

### Phase 4.6: Meta-Agent Core (Week 6)
- [ ] Create `configs/meta_agent.yaml` with the "Architect" persona.
- [ ] Implement `backend/api/meta_agent.py` with `/generate` and `/refine` endpoints.
- [ ] Develop the internal generation pipeline (Analysis -> Drafting -> Validation).

### Phase 4.7: Template Library & Patterns (Week 7)
- [ ] Populate `kb/architecture/patterns/` with 3-5 base agent templates (ReAct, Coder, Reviewer).
- [ ] Implement `/api/v1/meta-agent/templates` endpoint.

### Phase 4.8: Frontend Wizard Integration (Week 8)
- [ ] Build the "Agent Generation Wizard" UI components.
- [ ] Integrate the "Test Drive" sandbox feature.

## 7. Success Metrics
- [ ] Meta-Agent can generate a valid, runnable YAML config for 80% of common agent types (Coder, Reviewer, Researcher).
- [ ] Average generation time is under 15 seconds.
- [ ] Users can successfully refine a generated agent via natural language feedback.

[ProjectStatus] follow_up_needed
@!Coder Please implement the Meta-Agent module as described in this specification.
1. Start with `backend/api/meta_agent.py` and the core generation logic.
2. Create the `configs/meta_agent.yaml` definition.
3. Ensure the generated configs are compatible with the existing `src/config.rs` parser.
Deliverables:
- `backend/api/meta_agent.py`
- `configs/meta_agent.yaml`
- `kb/architecture/patterns/` (initial templates)
