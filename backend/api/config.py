"""Configuration validation endpoint."""

import yaml
from fastapi import APIRouter, Body
from typing import Any, Dict, List

from backend.core.rust_validator import validate_with_rust
from backend.models.schemas import ValidationResult, ExperimentConfig

router = APIRouter(prefix="/config", tags=["configuration"])


def validate_yaml_structure(config_dict: Dict[str, Any]) -> List[str]:
    """Validate the basic structure of the agent configuration."""
    errors: List[str] = []

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

    if "plugins" in config_dict and not isinstance(config_dict["plugins"], dict):
        errors.append("'plugins' section must be a mapping")

    if "hooks" in config_dict:
        hooks = config_dict["hooks"]
        if not isinstance(hooks, (list, dict)):
            errors.append("'hooks' section must be a list or mapping")

    return errors


def validate_config_semantics(config_dict: Dict[str, Any]) -> List[str]:
    """Validate semantic correctness of the configuration."""
    warnings: List[str] = []

    agent = config_dict.get("agent", {})
    if isinstance(agent, dict) and "strategy" in agent:
        warnings.append("'agent.strategy' is deprecated. Use top-level 'strategy' instead.")

    strategy = config_dict.get("strategy", {})
    if isinstance(strategy, dict) and "strategy" in strategy:
        warnings.append("'strategy.strategy' is deprecated. Use 'strategy.type' instead.")

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


def validate_config_dict(config_dict: Dict[str, Any]) -> ValidationResult:
    """Shared validation pipeline for API and meta-agent generation."""
    errors = validate_yaml_structure(config_dict)
    if errors:
        return ValidationResult(valid=False, errors=errors)

    warnings = validate_config_semantics(config_dict)

    try:
        parsed_config = ExperimentConfig(**config_dict)
    except Exception as exc:
        return ValidationResult(valid=False, errors=[f"Schema validation error: {exc}"])

    return ValidationResult(
        valid=True,
        errors=[],
        warnings=warnings,
        parsed_config=parsed_config,
    )


@router.post("/validate", response_model=ValidationResult)
async def validate_config(config_yaml: str = Body(default="", media_type="text/plain")):
    """Validate a YAML configuration string without running the experiment."""
    errors: List[str] = []
    warnings: List[str] = []

    try:
        config_dict = yaml.safe_load(config_yaml)
        if config_dict is None:
            return ValidationResult(valid=False, errors=["Empty configuration"])
        if not isinstance(config_dict, dict):
            return ValidationResult(valid=False, errors=["Configuration must be a YAML mapping"])
    except yaml.YAMLError as exc:
        return ValidationResult(valid=False, errors=[f"YAML parsing error: {exc}"])

    result = validate_config_dict(config_dict)
    if not result.valid:
        return result

    rust_ok, rust_errors = await validate_with_rust(config_yaml)
    if not rust_ok:
        return ValidationResult(
            valid=False,
            errors=rust_errors,
            warnings=result.warnings,
            parsed_config=result.parsed_config,
        )

    return ValidationResult(
        valid=True,
        errors=[],
        warnings=result.warnings,
        parsed_config=result.parsed_config,
    )
