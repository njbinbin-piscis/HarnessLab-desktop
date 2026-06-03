"""Meta-Agent endpoints for agent configuration generation."""

import uuid

from fastapi import APIRouter, HTTPException

from backend.core.meta_generator import (
    generate_agent_config,
    list_templates,
    refine_config,
)
from backend.models.schemas import (
    MetaAgentGenerateRequest,
    MetaAgentGenerateResponse,
    MetaAgentRefineRequest,
    MetaAgentRefineResponse,
)

router = APIRouter(prefix="/meta-agent", tags=["meta-agent"])


@router.post("/generate", response_model=MetaAgentGenerateResponse)
async def generate_agent(request: MetaAgentGenerateRequest):
    """Generate an agent YAML draft from wizard inputs."""
    if not request.goal_description.strip():
        raise HTTPException(status_code=400, detail="goal_description is required")

    config_yaml, system_prompt, validation = generate_agent_config(request)
    draft_id = f"draft_{uuid.uuid4().hex[:8]}"

    return MetaAgentGenerateResponse(
        status="success",
        draft_id=draft_id,
        config_yaml=config_yaml,
        system_prompt=system_prompt,
        validation_status={
            "valid": validation.valid,
            "errors": validation.errors,
            "warnings": validation.warnings,
        },
        suggested_tools=request.allowed_tools,
        confidence_score=0.85 if validation.valid else 0.5,
    )


@router.post("/refine", response_model=MetaAgentRefineResponse)
async def refine_agent(request: MetaAgentRefineRequest):
    """Refine an existing draft using natural-language feedback."""
    if not request.config_yaml.strip():
        raise HTTPException(status_code=400, detail="config_yaml is required")
    if not request.feedback.strip():
        raise HTTPException(status_code=400, detail="feedback is required")

    config_yaml, system_prompt, validation = refine_config(
        request.config_yaml,
        request.feedback,
        request.updated_tools,
    )

    return MetaAgentRefineResponse(
        status="success" if validation.valid else "needs_review",
        config_yaml=config_yaml,
        system_prompt=system_prompt,
        validation_status={
            "valid": validation.valid,
            "errors": validation.errors,
            "warnings": validation.warnings,
        },
    )


@router.get("/templates")
async def get_templates():
    """List available agent design patterns."""
    return list_templates()
