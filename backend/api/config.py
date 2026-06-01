"""Configuration validation endpoint."""

import yaml
from fastapi import APIRouter, HTTPException, Body
from typing import Dict, Any

from backend.models.schemas import ValidationResult, ExperimentConfig

router = APIRouter(prefix="/config", tags=["configuration"])


def validate_yaml_structure(config_dict: Dict[str, Any]) -> list:
    """Validate the basic structure of the agent configuration."""
    errors = []
    
    # Check required top-level keys
    if "agent" not in config_dict:
        errors.append("Missing required 'agent' section")
    else:
        agent = config_dict["agent"]
        if not isinstance(agent, dict):
            errors.append("'agent' section must be a mapping")
        else:
            if "name" not in agent:
                errors.append("Missing required 'agent.name' field")
            if "model" not in agent and "llm" not in agent:
                errors.append("Missing required 'agent.model' or 'agent.llm' field")
    
    # Validate plugins section if present
    if "plugins" in config_dict:
        if not isinstance(config_dict["plugins"], dict):
            errors.append("'plugins' section must be a mapping")
    
    # Validate hooks section if present
    if "hooks" in config_dict:
        if not isinstance(config_dict["hooks"], dict):
            errors.append("'hooks' section must be a mapping")
    
    return errors


def validate_config_semantics(config_dict: Dict[str, Any]) -> list:
    """Validate semantic correctness of the configuration."""
    warnings = []
    
    # Check for deprecated fields
    if "strategy" in config_dict.get("agent", {}):
        warnings.append(
            "'agent.strategy' is deprecated. Use 'context_manager' instead."
        )
    
    # Check for common mistakes
    agent = config_dict.get("agent", {})
    if isinstance(agent, dict):
        temperature = agent.get("temperature")
        if temperature is not None:
            try:
                temp_val = float(temperature)
                if temp_val < 0 or temp_val > 2:
                    warnings.append(
                        f"'agent.temperature' value {temp_val} is outside recommended range [0, 1]"
                    )
            except (ValueError, TypeError):
                warnings.append("'agent.temperature' should be a numeric value")
    
    return warnings


@router.post("/validate", response_model=ValidationResult)
async def validate_config(config_yaml: str = Body(default="", media_type="text/plain")):
    """
    Validate a YAML configuration string without running the experiment.
    
    Args:
        config_yaml: Raw YAML string containing agent configuration
        
    Returns:
        ValidationResult with validation status, errors, warnings, and parsed config
    """
    errors = []
    warnings = []
    parsed_config = None
    
    # Step 1: Parse YAML
    try:
        config_dict = yaml.safe_load(config_yaml)
        if config_dict is None:
            errors.append("Empty configuration")
            return ValidationResult(valid=False, errors=errors)
        if not isinstance(config_dict, dict):
            errors.append("Configuration must be a YAML mapping")
            return ValidationResult(valid=False, errors=errors)
    except yaml.YAMLError as e:
        errors.append(f"YAML parsing error: {str(e)}")
        return ValidationResult(valid=False, errors=errors)
    
    # Step 2: Validate structure
    structural_errors = validate_yaml_structure(config_dict)
    errors.extend(structural_errors)
    
    # If structural errors exist, return early
    if errors:
        return ValidationResult(valid=False, errors=errors)
    
    # Step 3: Validate semantics (only if structure is valid)
    warnings = validate_config_semantics(config_dict)
    
    # Step 4: Try to parse into Pydantic model
    try:
        parsed_config = ExperimentConfig(**config_dict)
    except Exception as e:
        errors.append(f"Schema validation error: {str(e)}")
        return ValidationResult(valid=False, errors=errors)
    
    return ValidationResult(
        valid=True,
        errors=[],
        warnings=warnings,
        parsed_config=parsed_config
    )
