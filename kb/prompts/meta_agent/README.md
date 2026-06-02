# Meta-Agent Prompt Templates

This directory contains the prompt templates and design specifications for the Meta-Agent module, which enables users to create custom agents through natural language descriptions.

## Directory Structure

```
kb/prompts/meta_agent/
├── README.md                              # This file
├── interaction_wizard_design.md           # Frontend wizard UI/UX specification
├── prompt_templates.md                    # LLM prompt template library
├── api_logic_specification.md             # Backend API implementation spec
└── templates/                             # Actual template files (to be created by Coder)
    ├── 01_analysis/
    ├── 02_generation/
    ├── 03_validation/
    ├── 04_refinement/
    └── agent_types/
```

## Design Documents

### 1. Interaction Wizard Design (`interaction_wizard_design.md`)

Specifies the 6-step frontend wizard for agent creation:

1. **Goal Description**: User describes the agent in natural language
2. **Constraint Selection**: Model preference, tool permissions, execution limits
3. **Tool Recommendation**: Meta-Agent suggests tools, user can override
4. **Configuration Preview**: Split-view YAML + system prompt editor with live validation
5. **Test Drive**: Sandbox execution to verify agent behavior
6. **Save & Deploy**: Finalize metadata and choose deployment option

**Key Features**:
- Real-time validation against existing config schema
- Auto-save to localStorage
- Accessibility support (keyboard navigation, screen readers)
- Error handling with specific feedback

### 2. Prompt Templates (`prompt_templates.md`)

Contains the LLM prompt templates used by the Meta-Agent internally:

#### Analysis Phase (01)
- `general_requirements.md`: Extract structured requirements from natural language
- `pattern_matching.md`: Match requirements to existing agent design patterns

#### Generation Phase (02)
- `yaml_config.md`: Generate complete YAML configuration
- `system_prompt.md`: Generate tailored system prompt

#### Validation Phase (03)
- `config_check.md`: Validate generated config against schema and best practices
- `error_correction.md`: Auto-correct common configuration errors

#### Refinement Phase (04)
- `feedback_integration.md`: Incorporate user feedback into existing config
- `optimization.md`: Optimize config for performance/cost/accuracy

#### Agent Type Templates
Pre-configured YAML templates for common agent types:
- `code_reviewer.yaml`: Code review agent with safety policies
- `data_analyst.yaml`: Data analysis with pandas/matplotlib
- `research_assistant.yaml`: Research with web search and citations

### 3. API Logic Specification (`api_logic_specification.md`)

Detailed backend API implementation guide:

#### Endpoints
- `POST /api/v1/meta-agent/generate`: Generate initial draft from description
- `POST /api/v1/meta-agent/refine`: Refine draft based on feedback
- `GET /api/v1/meta-agent/templates`: List available templates
- `POST /api/v1/meta-agent/test-drive`: Run sandbox test

#### Internal Logic
- Requirement analysis pipeline
- Pattern matching algorithm
- YAML generation with template rendering
- System prompt generation
- Config validation (reuses existing `/api/v1/config/validate`)
- Draft storage and management

#### Integration Points
- Existing LLM plugin system
- Experiment manager for sandbox tests
- Template library hot-reloading
- Draft cleanup job

## Usage for Implementation

### For Coder (Backend Implementation)

1. **Create API Module**: `backend/api/meta_agent.py`
   - Implement all 4 endpoints as specified
   - Reuse validation logic from `backend/api/config.py`
   - Integrate with existing LLM plugin system

2. **Create Meta-Agent Config**: `configs/meta_agent.yaml`
   - Define Meta-Agent's own persona ("Senior AI Architect")
   - Configure high-quality model (qwen-max)
   - Enable tools: file_read, file_write, yaml_validate

3. **Implement Helper Functions**:
   - `_analyze_requirements()`: Call LLM with analysis template
   - `_match_patterns()`: Search pattern library
   - `_generate_yaml_config()`: Render YAML template
   - `_generate_system_prompt()`: Render prompt template
   - `_validate_generated_config()`: Reuse existing validator
   - `_save_draft()` / `_load_draft()`: Draft persistence

4. **Create Template Files**:
   - Copy template content from `prompt_templates.md` into individual `.md` files
   - Organize in `kb/prompts/meta_agent/templates/` subdirectories
   - Create example inputs in `examples/` subdirectory

5. **Add Router Registration**: Update `backend/api/__init__.py` to include meta_agent router

6. **Write Tests**:
   - Unit tests for helper functions (mock LLM calls)
   - Integration tests for full generate → refine flow
   - End-to-end test: generate config, save, run experiment

### For Coder (Frontend Implementation)

1. **Create Wizard Component**: `frontend/src/components/MetaAgentWizard.tsx`
   - Implement 6-step wizard with state management
   - Use existing Monaco Editor for YAML editing
   - Reuse StreamLog component for test drive

2. **API Client**: `frontend/src/api/metaAgent.ts`
   - Wrapper functions for all 4 endpoints
   - Error handling with user-friendly messages
   - Retry logic for transient failures

3. **State Management**: Use React Context or Zustand for wizard state
   - Persist to localStorage
   - Restore on page reload

4. **Validation Integration**: Call `/api/v1/config/validate` on every edit (debounced 500ms)

5. **Sandbox Test**: Integrate with existing experiment streaming API

## Success Metrics

From the original Phase 4 specification:

- [ ] Meta-Agent can generate a valid, runnable YAML config for 80% of common agent types
- [ ] Average generation time is under 15 seconds
- [ ] Users can successfully refine a generated agent via natural language feedback
- [ ] Frontend wizard completes all 6 steps without errors
- [ ] Test drive sandbox correctly isolates experiments

## Next Steps

1. **Coder** implements backend API (`backend/api/meta_agent.py`)
2. **Coder** creates template files from specifications
3. **Coder** implements frontend wizard component
4. **Tester** validates generated configs work with existing engine
5. **Reviewer** reviews code for security and maintainability

## References

- Phase 4 Meta-Agent Module Architecture: `kb/architecture/phase4_meta_agent_module.md`
- Phase 4 Visualization Platform: `kb/architecture/phase4_visualization_platform.md`
- Core Architecture Design: `kb/architecture/core_architecture_design.md`
- Existing Config Validation: `backend/api/config.py`
- Existing Schemas: `backend/models/schemas.py`
