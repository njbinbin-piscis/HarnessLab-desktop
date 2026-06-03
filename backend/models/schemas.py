"""Pydantic models for API request/response and internal state."""

from datetime import datetime
from typing import Any, Dict, List, Literal, Optional
from pydantic import BaseModel, Field


class ExperimentConfig(BaseModel):
    """Represents the parsed YAML configuration for an agent experiment."""
    agent: Dict[str, Any] = Field(..., description="Agent configuration")
    plugins: Optional[Dict[str, Any]] = Field(default=None, description="Plugin configurations")
    policy: Optional[Dict[str, Any]] = Field(default=None, description="Policy settings")
    strategy: Optional[Dict[str, Any]] = Field(default=None, description="Strategy settings")
    hooks: Optional[List[Any]] = Field(default=None, description="Hook configurations")
    context_manager: Optional[str] = Field(default=None, description="Context manager plugin name")

    class Config:
        extra = "allow"


class MetaAgentGenerateRequest(BaseModel):
    """Wizard input for meta-agent generation."""
    goal_description: str = Field(..., description="Natural language agent goal")
    model_preference: str = Field(default="claude-sonnet-4.5")
    allowed_tools: List[str] = Field(default_factory=lambda: ["file_io"])
    strategy_type: str = Field(default="react")


class MetaAgentGenerateResponse(BaseModel):
    """Generated agent draft."""
    status: str
    draft_id: str
    config_yaml: str
    system_prompt: str
    validation_status: Dict[str, Any]
    suggested_tools: List[str] = Field(default_factory=list)
    confidence_score: float = 0.0


class MetaAgentRefineRequest(BaseModel):
    """Refinement request for an existing draft."""
    config_yaml: str
    feedback: str
    updated_tools: Optional[List[str]] = None


class MetaAgentRefineResponse(BaseModel):
    """Refined agent draft."""
    status: str
    config_yaml: str
    system_prompt: str
    validation_status: Dict[str, Any]


class ExperimentState(BaseModel):
    """Tracks the state of a running experiment."""
    id: str = Field(..., description="Unique experiment identifier")
    status: Literal["pending", "running", "completed", "failed"] = Field(
        default="pending", description="Current execution status"
    )
    config: ExperimentConfig = Field(..., description="Experiment configuration")
    start_time: Optional[datetime] = Field(default=None, description="Start timestamp")
    end_time: Optional[datetime] = Field(default=None, description="End timestamp")
    metrics: Dict[str, float] = Field(
        default_factory=dict, description="Performance metrics (tokens, duration, etc.)"
    )
    error_message: Optional[str] = Field(default=None, description="Error message if failed")


class AgentEvent(BaseModel):
    """Represents a single event in the agent execution loop."""
    timestamp: float = Field(..., description="Unix timestamp with milliseconds")
    type: Literal["thought", "action", "observation", "error", "complete"] = Field(
        ..., description="Event type"
    )
    content: str = Field(..., description="Event content/message")
    metadata: Optional[Dict[str, Any]] = Field(
        default=None, description="Additional event metadata (tool name, args, etc.)"
    )
    
    class Config:
        json_encoders = {
            datetime: lambda v: v.isoformat()
        }


class ValidationResult(BaseModel):
    """Result of YAML configuration validation."""
    valid: bool = Field(..., description="Whether the config is valid")
    errors: List[str] = Field(default_factory=list, description="List of validation errors")
    warnings: List[str] = Field(default_factory=list, description="List of warnings")
    parsed_config: Optional[ExperimentConfig] = Field(
        default=None, description="Parsed config if valid"
    )


class ExperimentRequest(BaseModel):
    """Request body for starting a new experiment."""
    config_yaml: str = Field(..., description="YAML configuration string")
    workspace_path: Optional[str] = Field(
        default=None, description="Optional workspace directory path"
    )
    prompt: Optional[str] = Field(
        default=None, description="Initial user prompt for the agent run"
    )


class ExperimentResponse(BaseModel):
    """Response body after starting an experiment."""
    experiment_id: str = Field(..., description="Unique experiment identifier")
    status: str = Field(..., description="Initial status (usually 'running')")


class SwarmAgentSpec(BaseModel):
    """One participant in a swarm: a name + its YAML config."""
    name: str = Field(..., description="Unique agent name within the swarm")
    config_yaml: str = Field(..., description="YAML configuration for this agent role")


class SwarmRequest(BaseModel):
    """Request body for starting a multi-agent swarm experiment."""
    task: str = Field(..., description="The shared task for the swarm to solve")
    agents: List[SwarmAgentSpec] = Field(
        ..., min_length=2, description="At least two participating agents"
    )
    coordinator: str = Field(
        default="majority_vote",
        description="Built-in coordinator name or 'module:Attr' / 'file.py:Attr'",
    )
    max_rounds: int = Field(default=8, ge=1, le=64)
    workspace_path: Optional[str] = Field(default=None)


class DatasetUploadRequest(BaseModel):
    """Register a user-provided validation set (JSONL task list)."""
    id: str = Field(..., description="Dataset id (alphanumeric / - / _)")
    content: str = Field(..., description="JSONL: one {task_id,prompt,answer,metadata} per line")
    name: Optional[str] = Field(default=None)
    scorer: str = Field(default="exact_match", description="Default scorer name")
    description: str = Field(default="")
    overwrite: bool = Field(default=False)


class DesignGenerateRequest(BaseModel):
    """Ask the Architect (or template fallback) to design an agent config."""
    goal_description: str = Field(..., description="Natural-language agent goal")
    model_preference: str = Field(default="claude-sonnet-4.5")
    allowed_tools: List[str] = Field(default_factory=lambda: ["file_io"])
    strategy_type: str = Field(default="react")
    workspace_path: Optional[str] = Field(default=None)


class PythonComponentRequest(BaseModel):
    """Author a Python coordinator/scorer component."""
    kind: Literal["coordinator", "scorer"]
    name: str
    code: str = Field(..., description="Module source defining the COMPONENT attribute")
    attr: str = Field(default="COMPONENT")
    description: str = Field(default="")


class RustStrategyRequest(BaseModel):
    """Author a Rust loop/compaction strategy (insert -> build -> register)."""
    kind: Literal["loop", "compaction"]
    name: str
    registration_code: str = Field(..., description="Body inserted into GENERATED REGISTRATIONS")
    items_code: str = Field(default="", description="Optional struct/impl inserted into GENERATED ITEMS")
    build: bool = Field(default=True, description="Run cargo build as the validation gate")
    description: str = Field(default="")


class RustLoopAliasRequest(BaseModel):
    """Author a new named loop strategy as a prompt-pattern alias (no new code)."""
    name: str
    build: bool = Field(default=True)


class AutoLoopRequest(BaseModel):
    """Start a self-improving design loop (propose -> evaluate -> revise)."""
    requirement: str = Field(..., description="Natural-language goal to design an agent for")
    dataset_id: str = Field(..., description="Validation set to score candidates against")
    rounds: int = Field(default=3, ge=1, le=20)
    target_accuracy: float = Field(default=1.0, ge=0.0, le=1.0)
    patience: int = Field(default=2, ge=1, le=10)
    strategy_type: str = Field(default="react")
    model: Optional[str] = Field(default=None, description="Pin candidate agent.model to this model")
    scorer: Optional[str] = Field(default=None)
    seeds: int = Field(default=1, ge=1, le=16)
    limit: Optional[int] = Field(default=None, ge=1)
    workspace_path: Optional[str] = Field(default=None)


class EvalRunRequest(BaseModel):
    """Start an evaluation run: an agent config over a validation set."""
    config_yaml: str = Field(..., description="Agent YAML config to evaluate")
    dataset_id: str = Field(..., description="Validation set id")
    agent_name: str = Field(default="Agent")
    scorer: Optional[str] = Field(default=None, description="Override the dataset's default scorer")
    seeds: int = Field(default=1, ge=1, le=16, description="Trials per task (pass^k)")
    limit: Optional[int] = Field(default=None, ge=1, description="Cap on number of tasks")
    workspace_path: Optional[str] = Field(default=None)
    label: Optional[str] = Field(default=None, description="Human label for comparison")
