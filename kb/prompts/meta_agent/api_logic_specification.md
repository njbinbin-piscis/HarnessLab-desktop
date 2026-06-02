# Meta-Agent API Logic Specification

## Overview
This document specifies the backend API implementation for the Meta-Agent module, including endpoint definitions, request/response schemas, internal processing logic, and integration points with existing systems.

## API Endpoints

### 1. POST /api/v1/meta-agent/generate

**Purpose**: Generate an initial agent configuration draft from a natural language description.

#### Request Schema

```python
class MetaAgentGenerateRequest(BaseModel):
    description: str = Field(
        ..., 
        min_length=20,
        max_length=2000,
        description="Natural language description of the desired agent"
    )
    constraints: Optional[MetaAgentConstraints] = Field(
        default=None,
        description="Optional constraints to guide generation"
    )
    template_style: Optional[str] = Field(
        default="react",
        description="Preferred agent strategy: react, plan-and-solve, or direct"
    )
    
class MetaAgentConstraints(BaseModel):
    model_preference: Optional[str] = Field(
        default=None,
        description="Preferred LLM model (e.g., qwen-plus, qwen-max)"
    )
    tools_allowed: Optional[List[str]] = Field(
        default=None,
        description="List of allowed tool names"
    )
    tools_denied: Optional[List[str]] = Field(
        default=None,
        description="List of explicitly denied tools"
    )
    max_turns: Optional[int] = Field(
        default=None,
        ge=1,
        le=50,
        description="Maximum number of agent turns"
    )
    temperature: Optional[float] = Field(
        default=None,
        ge=0.0,
        le=2.0,
        description="LLM temperature setting"
    )
```

#### Response Schema

```python
class MetaAgentGenerateResponse(BaseModel):
    status: Literal["success", "partial", "failed"] = Field(
        ..., 
        description="Generation status"
    )
    draft_id: str = Field(
        ..., 
        description="Unique identifier for this draft (for refinement)"
    )
    config_yaml: str = Field(
        ..., 
        description="Generated YAML configuration"
    )
    system_prompt: str = Field(
        ..., 
        description="Generated system prompt"
    )
    suggested_tools: List[str] = Field(
        default_factory=list,
        description="Tools recommended by the Meta-Agent"
    )
    confidence_score: float = Field(
        ..., 
        ge=0.0,
        le=1.0,
        description="Confidence in the generated configuration"
    )
    warnings: List[str] = Field(
        default_factory=list,
        description="Non-critical warnings about the configuration"
    )
    validation_errors: List[str] = Field(
        default_factory=list,
        description="Errors found during internal validation"
    )
```

#### Implementation Logic

```python
@router.post("/generate", response_model=MetaAgentGenerateResponse)
async def generate_agent(request: MetaAgentGenerateRequest):
    """
    Generate an agent configuration from natural language description.
    
    Flow:
    1. Validate input
    2. Analyze requirements using Meta-Agent LLM call
    3. Match to existing patterns
    4. Generate YAML config
    5. Generate system prompt
    6. Validate generated config
    7. Return result with draft_id
    """
    
    # Step 1: Input Validation
    if len(request.description.strip()) < 20:
        raise HTTPException(
            status_code=400,
            detail="Description must be at least 20 characters"
        )
    
    # Step 2: Requirement Analysis
    analysis_result = await _analyze_requirements(
        description=request.description,
        constraints=request.constraints
    )
    
    if analysis_result.confidence_score < 0.5:
        return MetaAgentGenerateResponse(
            status="partial",
            draft_id=_generate_draft_id(),
            config_yaml="",
            system_prompt="",
            suggested_tools=[],
            confidence_score=analysis_result.confidence_score,
            warnings=["Low confidence in requirement analysis. Please provide more details."],
            validation_errors=[]
        )
    
    # Step 3: Pattern Matching
    matched_patterns = await _match_patterns(analysis_result.requirements)
    
    # Step 4: Generate YAML Configuration
    config_yaml = await _generate_yaml_config(
        requirements=analysis_result.requirements,
        pattern=matched_patterns[0] if matched_patterns else None,
        template_style=request.template_style
    )
    
    # Step 5: Generate System Prompt
    system_prompt = await _generate_system_prompt(
        requirements=analysis_result.requirements,
        config_yaml=config_yaml
    )
    
    # Step 6: Internal Validation
    validation_result = await _validate_generated_config(config_yaml)
    
    # Step 7: Create Draft Record
    draft_id = _generate_draft_id()
    await _save_draft(
        draft_id=draft_id,
        config_yaml=config_yaml,
        system_prompt=system_prompt,
        requirements=analysis_result.requirements
    )
    
    # Step 8: Build Response
    status = "success" if validation_result.valid else "partial"
    
    return MetaAgentGenerateResponse(
        status=status,
        draft_id=draft_id,
        config_yaml=config_yaml,
        system_prompt=system_prompt,
        suggested_tools=analysis_result.requirements.required_tools,
        confidence_score=analysis_result.confidence_score,
        warnings=validation_result.warnings,
        validation_errors=validation_result.errors
    )
```

#### Helper Functions

```python
async def _analyze_requirements(
    description: str, 
    constraints: Optional[MetaAgentConstraints]
) -> RequirementAnalysis:
    """
    Use Meta-Agent LLM to analyze the description and extract requirements.
    
    Loads template from kb/prompts/meta_agent/templates/01_analysis/general_requirements.md
    """
    template = _load_template("01_analysis/general_requirements.md")
    prompt = template.replace("{{user_description}}", description)
    
    if constraints:
        prompt += f"\n\nAdditional Constraints:\n{constraints.json()}"
    
    # Call LLM via existing plugin system
    llm_response = await _call_meta_agent_llm(prompt)
    
    # Parse JSON response
    try:
        requirements = RequirementAnalysis.parse_raw(llm_response)
    except ValidationError as e:
        # Fallback: create minimal requirements
        requirements = RequirementAnalysis(
            role="GeneralAssistant",
            capabilities=["answer_questions"],
            required_tools=[],
            optional_tools=[],
            constraints={},
            domain="general",
            output_format="text",
            confidence_score=0.5
        )
    
    return requirements


async def _match_patterns(requirements: RequirementAnalysis) -> List[PatternMatch]:
    """
    Match requirements to existing agent design patterns.
    
    Searches kb/architecture/patterns/ for matching templates.
    """
    patterns = _load_pattern_library()
    
    template = _load_template("01_analysis/pattern_matching.md")
    prompt = template.replace("{{requirements_json}}", requirements.json())
    prompt = prompt.replace("{{pattern_library}}", json.dumps(patterns))
    
    llm_response = await _call_meta_agent_llm(prompt)
    
    try:
        matches = List[PatternMatch].parse_raw(llm_response)
        return matches
    except ValidationError:
        return []


async def _generate_yaml_config(
    requirements: RequirementAnalysis,
    pattern: Optional[PatternMatch],
    template_style: str
) -> str:
    """
    Generate YAML configuration using template.
    """
    template = _load_template("02_generation/yaml_config.md")
    
    # Prepare variables
    variables = {
        "agent_name": _derive_agent_name(requirements.role),
        "agent_description": requirements.capabilities[0] if requirements.capabilities else "General purpose agent",
        "model_preference": requirements.constraints.get("model", "qwen-plus"),
        "temperature": requirements.constraints.get("temperature", 0.7),
        "max_turns": requirements.constraints.get("max_turns", 10),
        "timeout": 30,
        "strategy": _map_strategy(template_style),
        "llm_provider": "qwen",
        "model_name": requirements.constraints.get("model", "qwen-plus"),
        "api_key_env_var": "QWEN_API_KEY",
        "memory_type": "sliding_window",
        "max_context_tokens": 8000,
        "enabled_tools": _format_tools_for_yaml(requirements.required_tools),
        "strictness": requirements.constraints.get("strictness", 0.8),
        "allow_shell": False,  # Default to safe
    }
    
    prompt = _render_template(template, variables)
    llm_response = await _call_meta_agent_llm(prompt)
    
    # Extract YAML from response (remove markdown code blocks if present)
    yaml_content = _extract_yaml(llm_response)
    
    return yaml_content


async def _generate_system_prompt(
    requirements: RequirementAnalysis,
    config_yaml: str
) -> str:
    """
    Generate system prompt using template.
    """
    template = _load_template("02_generation/system_prompt.md")
    
    variables = {
        "agent_role": requirements.role,
        "agent_description": requirements.capabilities[0] if requirements.capabilities else "General purpose agent",
        "capabilities_list": "\n".join(f"- {cap}" for cap in requirements.capabilities),
        "tools_list": "\n".join(f"- {tool}: [Use for {tool} operations]" for tool in requirements.required_tools),
        "constraints_summary": json.dumps(requirements.constraints, indent=2),
        "domain_knowledge": requirements.domain,
    }
    
    prompt = _render_template(template, variables)
    llm_response = await _call_meta_agent_llm(prompt)
    
    # Clean up response
    system_prompt = _clean_prompt_output(llm_response)
    
    return system_prompt


async def _validate_generated_config(config_yaml: str) -> ValidationResult:
    """
    Validate the generated config using existing validation endpoint logic.
    """
    # Reuse existing validation logic from backend/api/config.py
    errors = []
    warnings = []
    
    try:
        config_dict = yaml.safe_load(config_yaml)
        if not isinstance(config_dict, dict):
            errors.append("Generated config is not a valid YAML mapping")
            return ValidationResult(valid=False, errors=errors, warnings=warnings)
        
        # Structural validation
        structural_errors = validate_yaml_structure(config_dict)
        errors.extend(structural_errors)
        
        if errors:
            return ValidationResult(valid=False, errors=errors, warnings=warnings)
        
        # Semantic validation
        warnings = validate_config_semantics(config_dict)
        
        # Try Pydantic parsing
        try:
            ExperimentConfig(**config_dict)
        except Exception as e:
            errors.append(f"Schema validation error: {str(e)}")
            return ValidationResult(valid=False, errors=errors, warnings=warnings)
        
    except yaml.YAMLError as e:
        errors.append(f"YAML parsing error: {str(e)}")
        return ValidationResult(valid=False, errors=errors, warnings=warnings)
    
    return ValidationResult(valid=True, errors=[], warnings=warnings)


def _generate_draft_id() -> str:
    """Generate unique draft ID."""
    return f"draft_{uuid.uuid4().hex[:8]}"


async def _save_draft(
    draft_id: str,
    config_yaml: str,
    system_prompt: str,
    requirements: RequirementAnalysis
):
    """
    Save draft to temporary storage for later refinement.
    
    Storage options:
    - In-memory cache (short-lived)
    - Redis (if available)
    - File system in backend/drafts/ directory
    """
    draft_data = {
        "draft_id": draft_id,
        "config_yaml": config_yaml,
        "system_prompt": system_prompt,
        "requirements": requirements.dict(),
        "created_at": datetime.utcnow().isoformat(),
        "expires_at": (datetime.utcnow() + timedelta(hours=24)).isoformat()
    }
    
    # Save to drafts directory
    draft_path = Path("backend/drafts") / f"{draft_id}.json"
    draft_path.parent.mkdir(parents=True, exist_ok=True)
    
    with open(draft_path, "w") as f:
        json.dump(draft_data, f, indent=2)


def _call_meta_agent_llm(prompt: str) -> str:
    """
    Call the LLM using the Meta-Agent's configured model.
    
    Uses the existing plugin system to make LLM calls.
    The Meta-Agent itself is configured with a high-quality model (qwen-max).
    """
    # This should use the existing LLM plugin infrastructure
    # For now, placeholder implementation
    raise NotImplementedError("Integrate with existing LLM plugin system")
```


### 2. POST /api/v1/meta-agent/refine

**Purpose**: Refine an existing draft based on user feedback or updated constraints.

#### Request Schema

```python
class MetaAgentRefineRequest(BaseModel):
    draft_id: str = Field(..., description="ID of the draft to refine")
    feedback: str = Field(
        ..., 
        min_length=10,
        description="User feedback describing desired changes"
    )
    updated_constraints: Optional[MetaAgentConstraints] = Field(
        default=None,
        description="Updated constraints to apply"
    )
```

#### Response Schema

Same as `MetaAgentGenerateResponse`, but with updated content.

#### Implementation Logic

```python
@router.post("/refine", response_model=MetaAgentGenerateResponse)
async def refine_agent(request: MetaAgentRefineRequest):
    """
    Refine an existing agent draft based on feedback.
    
    Flow:
    1. Load existing draft
    2. Analyze feedback
    3. Apply changes to config and prompt
    4. Validate updated config
    5. Save new draft version
    6. Return updated result
    """
    
    # Step 1: Load Draft
    draft_data = await _load_draft(request.draft_id)
    if not draft_data:
        raise HTTPException(status_code=404, detail="Draft not found")
    
    # Step 2: Analyze Feedback
    refinement_plan = await _analyze_feedback(
        current_config=draft_data["config_yaml"],
        current_prompt=draft_data["system_prompt"],
        feedback=request.feedback,
        updated_constraints=request.updated_constraints
    )
    
    # Step 3: Apply Changes
    updated_config = await _apply_refinements(
        original_config=draft_data["config_yaml"],
        original_prompt=draft_data["system_prompt"],
        refinement_plan=refinement_plan
    )
    
    # Step 4: Validate
    validation_result = await _validate_generated_config(updated_config["config_yaml"])
    
    # Step 5: Save New Draft Version
    new_draft_id = _generate_draft_id()
    await _save_draft(
        draft_id=new_draft_id,
        config_yaml=updated_config["config_yaml"],
        system_prompt=updated_config["system_prompt"],
        requirements=draft_data["requirements"]  # Keep original requirements
    )
    
    # Step 6: Build Response
    status = "success" if validation_result.valid else "partial"
    
    return MetaAgentGenerateResponse(
        status=status,
        draft_id=new_draft_id,
        config_yaml=updated_config["config_yaml"],
        system_prompt=updated_config["system_prompt"],
        suggested_tools=_extract_tools_from_config(updated_config["config_yaml"]),
        confidence_score=refinement_plan.confidence_score,
        warnings=validation_result.warnings,
        validation_errors=validation_result.errors
    )
```


### 3. GET /api/v1/meta-agent/templates

**Purpose**: Retrieve available agent design patterns/templates.

#### Response Schema

```python
class AgentTemplate(BaseModel):
    id: str = Field(..., description="Template identifier")
    name: str = Field(..., description="Human-readable name")
    description: str = Field(..., description="Template description")
    category: str = Field(..., description="Category (e.g., coding, analysis, research)")
    preview_yaml: str = Field(..., description="Short preview of the config")
    use_count: int = Field(default=0, description="Number of times this template was used")

@router.get("/templates", response_model=List[AgentTemplate])
async def list_templates():
    """
    List all available agent templates.
    
    Scans kb/architecture/patterns/ and kb/prompts/meta_agent/templates/agent_types/
    """
    templates = []
    
    # Load from agent_types directory
    templates_dir = Path("kb/prompts/meta_agent/templates/agent_types")
    if templates_dir.exists():
        for template_file in templates_dir.glob("*.yaml"):
            with open(template_file) as f:
                config = yaml.safe_load(f)
            
            templates.append(AgentTemplate(
                id=template_file.stem,
                name=config.get("agent", {}).get("name", template_file.stem),
                description=config.get("agent", {}).get("description", ""),
                category=_categorize_template(template_file.stem),
                preview_yaml=yaml.dump(config, default_flow_style=False)[:200],
                use_count=_get_template_use_count(template_file.stem)
            ))
    
    # Sort by use_count descending
    templates.sort(key=lambda t: t.use_count, reverse=True)
    
    return templates
```


### 4. POST /api/v1/meta-agent/test-drive

**Purpose**: Run a short sandbox experiment to test the generated agent.

#### Request Schema

```python
class MetaAgentTestDriveRequest(BaseModel):
    config_yaml: str = Field(..., description="YAML configuration to test")
    system_prompt: str = Field(..., description="System prompt to use")
    test_scenario: str = Field(
        ..., 
        description="Test prompt to send to the agent"
    )
    max_turns: int = Field(
        default=3,
        ge=1,
        le=5,
        description="Maximum turns for test (limited for sandbox)"
    )
```

#### Response Schema

```python
class MetaAgentTestDriveResponse(BaseModel):
    success: bool = Field(..., description="Whether test completed successfully")
    events: List[AgentEvent] = Field(..., description="Execution events")
    metrics: Dict[str, float] = Field(..., description="Performance metrics")
    result_summary: str = Field(..., description="Brief summary of test outcome")
    suggestions: List[str] = Field(
        default_factory=list,
        description="Suggestions for improvement if test failed"
    )
```

#### Implementation Logic

```python
@router.post("/test-drive", response_model=MetaAgentTestDriveResponse)
async def test_drive_agent(request: MetaAgentTestDriveRequest):
    """
    Run a sandboxed test of the agent configuration.
    
    Flow:
    1. Validate config
    2. Create isolated experiment context
    3. Run agent with limited turns
    4. Collect events and metrics
    5. Analyze results
    6. Return summary
    """
    
    # Step 1: Validate Config
    validation = await _validate_generated_config(request.config_yaml)
    if not validation.valid:
        raise HTTPException(
            status_code=400,
            detail=f"Invalid configuration: {validation.errors}"
        )
    
    # Step 2: Create Sandbox Context
    experiment_id = f"sandbox_{uuid.uuid4().hex[:8]}"
    sandbox_config = _create_sandbox_config(
        config_yaml=request.config_yaml,
        system_prompt=request.system_prompt,
        max_turns=request.max_turns
    )
    
    # Step 3: Run Experiment
    events = []
    metrics = {}
    
    try:
        events, metrics = await _run_sandbox_experiment(
            experiment_id=experiment_id,
            config=sandbox_config,
            test_prompt=request.test_scenario
        )
        
        success = _evaluate_test_success(events, metrics)
        result_summary = _generate_test_summary(events, success)
        suggestions = _generate_suggestions(events, metrics) if not success else []
        
    except Exception as e:
        success = False
        result_summary = f"Test failed with error: {str(e)}"
        suggestions = ["Check configuration for errors", "Try a simpler test scenario"]
        events = []
        metrics = {"error": 1}
    
    return MetaAgentTestDriveResponse(
        success=success,
        events=events,
        metrics=metrics,
        result_summary=result_summary,
        suggestions=suggestions
    )
```


## Internal Data Models

```python
class RequirementAnalysis(BaseModel):
    """Parsed requirements from user description."""
    role: str
    capabilities: List[str]
    required_tools: List[str]
    optional_tools: List[str]
    constraints: Dict[str, Any]
    domain: str
    output_format: str
    confidence_score: float


class PatternMatch(BaseModel):
    """Match between requirements and existing pattern."""
    pattern_id: str
    similarity: float
    reasoning: str
    adaptations: List[str]


class RefinementPlan(BaseModel):
    """Plan for refining a configuration."""
    changes: List[Dict[str, str]]
    confidence_score: float
    clarification_needed: Optional[List[str]] = None
```


## Integration Points

### 1. Existing Validation Endpoint
- Reuse `POST /api/v1/config/validate` logic for config validation
- Import functions from `backend/api/config.py`

### 2. LLM Plugin System
- Use existing LLM plugin infrastructure for Meta-Agent's own LLM calls
- Meta-Agent should use a high-quality model (qwen-max) for generation tasks

### 3. Experiment Manager
- Reuse `backend/core/manager.py` for running sandbox tests
- Create isolated experiment contexts with resource limits

### 4. Template Library
- Load templates from `kb/prompts/meta_agent/templates/`
- Support hot-reloading of templates without server restart

### 5. Draft Storage
- Store drafts in `backend/drafts/` directory
- Implement cleanup job to remove expired drafts (>24 hours)


## Error Handling

### Common Errors

| Error Code | Scenario | Response |
|------------|----------|----------|
| 400 | Invalid input (description too short, invalid constraints) | Detailed error message |
| 404 | Draft not found | "Draft not found" |
| 422 | Validation failure | List of validation errors |
| 500 | LLM call failure | "Generation service unavailable, please retry" |
| 503 | Timeout (generation > 30s) | "Generation timed out, please simplify your request" |

### Retry Logic
- LLM calls: Retry up to 3 times with exponential backoff (1s, 2s, 4s)
- Template loading: No retry (fail fast if template missing)
- Draft saving: Retry once if file write fails


## Security Considerations

1. **Input Sanitization**: Validate all user inputs to prevent injection attacks
2. **Resource Limits**: Enforce max turns, timeout, and token limits in sandbox
3. **Tool Permissions**: Never enable dangerous tools (file_delete, shell_command) by default
4. **Draft Expiration**: Automatically delete drafts after 24 hours
5. **Rate Limiting**: Limit generate/refine calls to 10 per minute per user


## Performance Targets

- **Generation Time**: < 15 seconds (p95)
- **Refinement Time**: < 10 seconds (p95)
- **Template Listing**: < 100ms
- **Test Drive**: < 30 seconds (p95, includes agent execution)


## Testing Strategy

### Unit Tests
- Test each helper function independently
- Mock LLM calls with fixed responses
- Validate template rendering

### Integration Tests
- Test full generate → refine → test-drive flow
- Verify draft persistence and retrieval
- Test error handling paths

### End-to-End Tests
- Create agent via API, save to file, run actual experiment
- Verify generated configs work with existing engine
