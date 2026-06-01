"""Pydantic models for API request/response and internal state."""

from datetime import datetime
from typing import Any, Dict, List, Literal, Optional
from pydantic import BaseModel, Field


class ExperimentConfig(BaseModel):
    """Represents the parsed YAML configuration for an agent experiment."""
    agent: Dict[str, Any] = Field(..., description="Agent configuration")
    plugins: Optional[Dict[str, Any]] = Field(default=None, description="Plugin configurations")
    hooks: Optional[Dict[str, Any]] = Field(default=None, description="Hook configurations")
    context_manager: Optional[str] = Field(default=None, description="Context manager plugin name")
    
    class Config:
        extra = "allow"


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


class ExperimentResponse(BaseModel):
    """Response body after starting an experiment."""
    experiment_id: str = Field(..., description="Unique experiment identifier")
    status: str = Field(..., description="Initial status (usually 'running')")
