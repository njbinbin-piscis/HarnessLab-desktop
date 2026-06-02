# Meta-Agent Prompt Template Library

## Overview
This directory contains prompt templates used by the Meta-Agent to generate YAML configurations and system prompts for custom agents. Templates are organized by agent type and generation phase.

## Template Structure

Each template file follows this naming convention:
- `{phase}_{agent_type}_{purpose}.md`

Phases:
- `01_analysis`: Requirement analysis and pattern matching
- `02_generation`: YAML config and system prompt generation
- `03_validation`: Config validation and error correction
- `04_refinement`: Iterative refinement based on feedback


## 01 Analysis Phase Templates

### 01_analysis_general_requirements.md
**Purpose**: Analyze natural language description to extract agent requirements.

```markdown
# Agent Requirement Analysis

You are an AI Architect analyzing a user's request to create a custom agent.

## User Description
{{user_description}}

## Task
Analyze the description and extract:

1. **Primary Role**: What is the agent's main purpose? (e.g., Code Reviewer, Data Analyst)
2. **Key Capabilities**: What specific tasks must it perform?
3. **Required Tools**: Which tools are essential? (file_read, code_run, web_search, etc.)
4. **Constraints**: Any explicit limitations (max turns, model preference, safety rules)?
5. **Domain Knowledge**: What specialized knowledge does it need?
6. **Output Format**: What should the agent produce? (code, reports, answers, etc.)

## Output Format
Return a JSON object with these fields:
{
  "role": "...",
  "capabilities": ["...", "..."],
  "required_tools": ["...", "..."],
  "optional_tools": ["...", "..."],
  "constraints": {...},
  "domain": "...",
  "output_format": "...",
  "confidence_score": 0.0-1.0
}

## Analysis Guidelines
- Be conservative with tool recommendations: only suggest tools clearly needed
- If the description is vague, note ambiguities in a "clarification_needed" field
- Prefer standard tools over custom ones unless explicitly requested
- Consider security implications of tool permissions
```


### 01_analysis_pattern_matching.md
**Purpose**: Match requirements to existing agent design patterns.

```markdown
# Agent Pattern Matching

You are matching agent requirements to known design patterns.

## Extracted Requirements
{{requirements_json}}

## Available Patterns
{{pattern_library}}

## Task
Find the best-matching pattern(s) from the library. For each match, provide:

1. **Pattern ID**: The identifier from the library
2. **Similarity Score**: 0.0-1.0
3. **Match Reasoning**: Why this pattern fits
4. **Adaptation Notes**: What needs to change from the base pattern

## Output Format
Return a JSON array sorted by similarity score (highest first):
[
  {
    "pattern_id": "...",
    "similarity": 0.92,
    "reasoning": "...",
    "adaptations": ["...", "..."]
  }
]

If no good match exists (all scores < 0.6), return an empty array and suggest creating a new pattern.
```


## 02 Generation Phase Templates

### 02_generation_yaml_config.md
**Purpose**: Generate the YAML configuration file for the agent.

```markdown
# Agent YAML Configuration Generator

You are generating a YAML configuration for a custom agent based on analyzed requirements.

## Requirements
{{requirements_json}}

## Selected Pattern
{{selected_pattern}}

## Available Tools
{{available_tools_list}}

## Task
Generate a complete YAML configuration following this structure:

```yaml
agent:
  name: {{agent_name}}
  description: {{agent_description}}
  model: {{model_preference}}
  temperature: {{temperature}}
  max_turns: {{max_turns}}
  timeout_per_turn: {{timeout}}
  
context_manager: {{strategy}}  # react, plan_and_solve, or direct

plugins:
  llm:
    provider: {{llm_provider}}
    model: {{model_name}}
    api_key_env: {{api_key_env_var}}
  
  memory:
    type: {{memory_type}}  # sliding_window, full_history, or vector_store
    max_tokens: {{max_context_tokens}}
  
  tools:
    enabled:
      {{#each enabled_tool}}
      - {{tool_name}}:
          config: {{tool_config}}
      {{/each}}

hooks:
  pre_action:
    - type: log_event
      config:
        level: info
  
  post_observation:
    - type: validate_output
      config:
        strict: {{strictness}}

policies:
  safety:
    allow_file_delete: false
    allow_shell_commands: {{allow_shell}}
    max_file_size_mb: 10
  
  resource_limits:
    max_total_tokens: 50000
    max_execution_time_seconds: 300
```

## Generation Rules
1. **Agent Name**: Derive from role (e.g., "PythonCodeReviewer")
2. **Model Selection**: Use the specified preference, default to qwen-plus
3. **Tool Configuration**: Only include tools from the required/optional lists
4. **Strategy Mapping**:
   - ReAct → `context_manager: react`
   - Plan-and-Solve → `context_manager: plan_and_solve`
   - Direct Answer → `context_manager: direct`
5. **Safety Defaults**: Always disable dangerous operations unless explicitly requested
6. **Validation**: Ensure all referenced tools exist in the available tools list

## Output
Return ONLY the YAML content, no markdown formatting, no explanations.
```


### 02_generation_system_prompt.md
**Purpose**: Generate the system prompt for the agent.

```markdown
# Agent System Prompt Generator

You are crafting a system prompt that will guide the agent's behavior.

## Agent Profile
- **Role**: {{agent_role}}
- **Description**: {{agent_description}}
- **Capabilities**: {{capabilities_list}}
- **Available Tools**: {{tools_list}}
- **Constraints**: {{constraints_summary}}
- **Domain**: {{domain_knowledge}}

## Task
Write a comprehensive system prompt that:

1. **Establishes Identity**: Clearly state the agent's role and expertise
2. **Defines Scope**: What the agent should and should not do
3. **Explains Tools**: Brief description of each available tool and when to use it
4. **Sets Expectations**: Response format, tone, and quality standards
5. **Encodes Constraints**: Safety rules, limits, and policies
6. **Provides Examples**: 1-2 short examples of ideal behavior (if applicable)

## Prompt Structure
```
You are [ROLE], a specialized AI assistant focused on [DOMAIN].

## Core Responsibilities
- [Responsibility 1]
- [Responsibility 2]
- ...

## Available Tools
You have access to the following tools:
- [Tool 1]: [When to use it]
- [Tool 2]: [When to use it]
...

## Guidelines
1. [Guideline 1]
2. [Guideline 2]
...

## Constraints
- [Constraint 1]
- [Constraint 2]
...

## Response Format
[Expected output format]

Remember: [Key principle or motto]
```

## Tone and Style
- Professional but approachable
- Clear and concise
- Action-oriented
- Avoid jargon unless domain-specific

## Output
Return ONLY the system prompt text, no markdown code blocks, no explanations.
```


## 03 Validation Phase Templates

### 03_validation_config_check.md
**Purpose**: Validate generated config against schema and best practices.

```markdown
# Configuration Validator

You are reviewing a generated agent configuration for correctness and best practices.

## Generated Configuration
{{config_yaml}}

## Original Requirements
{{requirements_json}}

## Validation Checklist

### Structural Validation
- [ ] All required fields present (agent.name, agent.model, context_manager)
- [ ] Valid YAML syntax
- [ ] No duplicate keys
- [ ] Proper indentation

### Semantic Validation
- [ ] Model name is valid and available
- [ ] Temperature is between 0.0 and 2.0
- [ ] Max turns is reasonable (1-50)
- [ ] All referenced tools exist in the tool registry
- [ ] Tool configurations match tool schemas

### Alignment Check
- [ ] Enabled tools match the required tools from requirements
- [ ] Strategy matches the requested approach
- [ ] Constraints are properly encoded in policies section
- [ ] System prompt aligns with agent role

### Security Review
- [ ] Dangerous tools (file_delete, shell_command) are disabled by default
- [ ] Resource limits are set
- [ ] No hardcoded API keys or secrets

## Output Format
Return a JSON object:
{
  "valid": true/false,
  "errors": [
    {
      "severity": "error" | "warning",
      "field": "path.to.field",
      "message": "...",
      "suggestion": "..."
    }
  ],
  "confidence_score": 0.0-1.0
}

If valid is false, explain what needs to be fixed.
If there are warnings, list them but still mark as valid if they're non-critical.
```


### 03_validation_error_correction.md
**Purpose**: Auto-correct common configuration errors.

```markdown
# Configuration Error Corrector

You are fixing errors in an agent configuration.

## Broken Configuration
{{config_yaml}}

## Validation Errors
{{errors_json}}

## Task
Fix the configuration to resolve all errors. For each error:

1. Identify the root cause
2. Apply the minimal fix needed
3. Ensure the fix doesn't introduce new issues

## Correction Rules
- **Missing Fields**: Add with sensible defaults
- **Invalid Values**: Replace with nearest valid value
- **Tool Mismatches**: Remove unavailable tools or replace with equivalents
- **Schema Violations**: Adjust to match expected schema
- **Security Issues**: Disable dangerous features

## Output
Return the corrected YAML configuration only, no explanations.

If errors cannot be automatically fixed (e.g., fundamental requirement conflict), 
return the original config and add a comment at the top:
```yaml
# CORRECTION_FAILED: [reason]
# Manual intervention required for: [list of issues]
```
```


## 04 Refinement Phase Templates

### 04_refinement_feedback_integration.md
**Purpose**: Incorporate user feedback into an existing agent configuration.

```markdown
# Agent Configuration Refiner

You are refining an agent configuration based on user feedback.

## Current Configuration
{{current_config_yaml}}

## Current System Prompt
{{current_system_prompt}}

## User Feedback
{{user_feedback}}

## Updated Constraints (if any)
{{updated_constraints_json}}

## Task
Modify the configuration and system prompt to address the feedback while preserving 
the core functionality.

## Refinement Strategy
1. **Parse Feedback**: Identify what the user wants to change
2. **Impact Analysis**: Determine which sections need modification
3. **Apply Changes**: Make targeted edits
4. **Consistency Check**: Ensure changes don't break existing functionality
5. **Validate**: Run mental validation against the checklist

## Change Types
- **Tool Addition/Removal**: Update plugins.tools.enabled section
- **Behavior Adjustment**: Modify system prompt and policies
- **Constraint Tightening/Loosening**: Update policies and resource_limits
- **Model Change**: Update agent.model and plugins.llm sections
- **Strategy Shift**: Change context_manager and adjust prompt accordingly

## Output Format
Return a JSON object:
{
  "updated_config_yaml": "...",
  "updated_system_prompt": "...",
  "changes_summary": [
    {
      "section": "...",
      "change_type": "add" | "remove" | "modify",
      "description": "...",
      "reason": "..."
    }
  ],
  "confidence_score": 0.0-1.0
}

If the feedback is unclear or conflicts with core requirements, note this in 
a "clarification_needed" field with specific questions.
```


### 04_refinement_optimization.md
**Purpose**: Optimize an existing configuration for better performance.

```markdown
# Agent Configuration Optimizer

You are optimizing an agent configuration for efficiency and effectiveness.

## Current Configuration
{{config_yaml}}

## Performance Metrics (if available)
{{metrics_json}}

## Optimization Goals
{{optimization_goals}}  # e.g., "reduce token usage", "improve speed", "increase accuracy"

## Task
Suggest optimizations that improve the specified goals without breaking functionality.

## Optimization Strategies

### Token Efficiency
- Reduce verbose instructions in system prompt
- Use concise tool descriptions
- Set appropriate max_context_tokens

### Speed Improvements
- Recommend faster model if accuracy allows
- Reduce max_turns if task is simple
- Enable caching if applicable

### Accuracy Improvements
- Add few-shot examples to system prompt
- Increase temperature for creative tasks, decrease for precise tasks
- Add validation hooks

### Cost Reduction
- Suggest cheaper model alternatives
- Implement early termination conditions
- Optimize tool usage patterns

## Output Format
Return a JSON object:
{
  "optimized_config_yaml": "...",
  "optimizations_applied": [
    {
      "category": "token_efficiency" | "speed" | "accuracy" | "cost",
      "change": "...",
      "expected_impact": "...",
      "trade_offs": "..."
    }
  ],
  "before_metrics_estimate": {...},
  "after_metrics_estimate": {...}
}
```


## Agent Type-Specific Templates

### templates/code_reviewer.yaml
**Pre-configured template for code review agents.**

```yaml
# Base template for Code Reviewer agents
agent:
  name: CodeReviewer
  description: Reviews code for quality, security, and best practices
  model: qwen-plus
  temperature: 0.3
  max_turns: 8
  
context_manager: react

plugins:
  llm:
    provider: qwen
    model: qwen-plus
  
  memory:
    type: sliding_window
    max_tokens: 8000
  
  tools:
    enabled:
      - file_read:
          allowed_patterns: ["*.py", "*.js", "*.ts", "*.java", "*.go"]
      - file_write:
          allowed_patterns: ["*.md", "*.txt"]  # Only for reports
      
hooks:
  pre_action:
    - type: validate_file_access
      config:
        check_permissions: true
  
  post_observation:
    - type: format_review_output
      config:
        include_line_numbers: true
        severity_levels: ["critical", "warning", "suggestion"]

policies:
  safety:
    allow_file_delete: false
    allow_shell_commands: false
    allow_code_execution: false  # Review only, no execution
  
  review_standards:
    check_pep8: true
    check_security: true
    check_performance: true
    require_explanations: true
```


### templates/data_analyst.yaml
**Pre-configured template for data analysis agents.**

```yaml
# Base template for Data Analyst agents
agent:
  name: DataAnalyst
  description: Analyzes data files and generates insights with visualizations
  model: qwen-plus
  temperature: 0.5
  max_turns: 12
  
context_manager: plan_and_solve

plugins:
  llm:
    provider: qwen
    model: qwen-plus
  
  memory:
    type: sliding_window
    max_tokens: 12000
  
  tools:
    enabled:
      - file_read:
          allowed_patterns: ["*.csv", "*.json", "*.xlsx", "*.parquet"]
      - code_run:
          allowed_languages: ["python"]
          allowed_packages: ["pandas", "numpy", "matplotlib", "seaborn"]
      - file_write:
          allowed_patterns: ["*.png", "*.jpg", "*.pdf", "*.md", "*.csv"]

hooks:
  pre_action:
    - type: validate_data_access
      config:
        max_file_size_mb: 100
  
  post_observation:
    - type: validate_chart_output
      config:
        require_labels: true
        require_title: true

policies:
  safety:
    allow_file_delete: false
    allow_shell_commands: false
    allow_network_access: false
  
  analysis_standards:
    handle_missing_data: true
    validate_assumptions: true
    include_statistical_tests: true
    generate_visualizations: true
```


### templates/research_assistant.yaml
**Pre-configured template for research assistant agents.**

```yaml
# Base template for Research Assistant agents
agent:
  name: ResearchAssistant
  description: Conducts literature reviews and synthesizes research findings
  model: qwen-max
  temperature: 0.7
  max_turns: 15
  
context_manager: plan_and_solve

plugins:
  llm:
    provider: qwen
    model: qwen-max
  
  memory:
    type: vector_store
    config:
      embedding_model: text-embedding-3-small
      max_documents: 50
  
  tools:
    enabled:
      - web_search:
          max_results: 10
      - file_read:
          allowed_patterns: ["*.pdf", "*.txt", "*.md"]
      - file_write:
          allowed_patterns: ["*.md", "*.bib", "*.txt"]

hooks:
  pre_action:
    - type: rate_limit_web_requests
      config:
        max_requests_per_minute: 10
  
  post_observation:
    - type: extract_citations
      config:
        format: "APA"

policies:
  safety:
    allow_file_delete: false
    allow_shell_commands: false
  
  research_standards:
    require_citations: true
    verify_sources: true
    avoid_plagiarism: true
    summarize_before_synthesizing: true
```


## Template Usage Guidelines

### For Meta-Agent Implementation

1. **Loading Templates**: 
   - Read template files from `kb/prompts/meta_agent/templates/`
   - Cache frequently used templates in memory
   - Support template versioning (e.g., `code_reviewer_v2.yaml`)

2. **Variable Substitution**:
   - Use Mustache-style `{{variable}}` syntax
   - Validate all variables are provided before rendering
   - Provide sensible defaults for optional variables

3. **Template Composition**:
   - Allow combining multiple templates (e.g., base + domain-specific)
   - Support template inheritance (child overrides parent)
   - Maintain a template dependency graph

4. **Testing Templates**:
   - Each template should have example inputs in `examples/` subdirectory
   - Validate generated configs pass schema validation
   - Test edge cases (empty inputs, conflicting constraints)

### File Organization

```
kb/prompts/meta_agent/
├── interaction_wizard_design.md      # This file's sibling
├── prompt_templates.md               # This file
├── templates/
│   ├── 01_analysis/
│   │   ├── general_requirements.md
│   │   └── pattern_matching.md
│   ├── 02_generation/
│   │   ├── yaml_config.md
│   │   └── system_prompt.md
│   ├── 03_validation/
│   │   ├── config_check.md
│   │   └── error_correction.md
│   ├── 04_refinement/
│   │   ├── feedback_integration.md
│   │   └── optimization.md
│   └── agent_types/
│       ├── code_reviewer.yaml
│       ├── data_analyst.yaml
│       └── research_assistant.yaml
├── examples/
│   ├── sample_description.txt
│   ├── sample_requirements.json
│   └── sample_generated_config.yaml
└── README.md                         # Usage instructions
```
