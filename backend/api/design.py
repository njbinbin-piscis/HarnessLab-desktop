"""Design-assistant endpoints (Pillar 2).

Exposes the Architect: generate agent configs (agent-driven with template
fallback) and author new pluggable harness components (Python hot-load, or Rust
with a build gate). Long Rust builds run off the event loop.
"""

import asyncio

from fastapi import APIRouter, HTTPException

from backend.core import component_builder as cb
from backend.core.design import generate_design, read_architect_config
from backend.models.schemas import (
    DesignGenerateRequest,
    MetaAgentGenerateRequest,
    PythonComponentRequest,
    RustLoopAliasRequest,
    RustStrategyRequest,
)

router = APIRouter(prefix="/design", tags=["design"])


@router.get("/architect")
async def get_architect():
    """Return the Architect's own (designable) YAML config."""
    try:
        return {"config_yaml": read_architect_config()}
    except OSError as exc:
        raise HTTPException(status_code=500, detail=f"cannot read architect config: {exc}")


@router.post("/generate")
async def design_generate(request: DesignGenerateRequest):
    """Design an agent config from a requirement (agent-driven or template)."""
    if not request.goal_description.strip():
        raise HTTPException(status_code=400, detail="goal_description is required")
    meta_req = MetaAgentGenerateRequest(
        goal_description=request.goal_description,
        model_preference=request.model_preference,
        allowed_tools=request.allowed_tools,
        strategy_type=request.strategy_type,
    )
    return await generate_design(meta_req, workspace=request.workspace_path)


@router.post("/components/python")
async def author_python(request: PythonComponentRequest):
    """Author + register a Python coordinator/scorer (no build needed)."""
    try:
        result = cb.author_python_component(
            kind=request.kind,
            name=request.name,
            code=request.code,
            attr=request.attr,
            description=request.description,
        )
    except cb.ComponentBuildError as exc:
        raise HTTPException(status_code=400, detail=str(exc))
    return _result_payload(result)


@router.post("/components/rust")
async def author_rust(request: RustStrategyRequest):
    """Author a Rust loop/compaction strategy with a cargo-build validation gate."""
    try:
        result = await asyncio.to_thread(
            cb.author_rust_strategy,
            name=request.name,
            kind=request.kind,
            registration_code=request.registration_code,
            items_code=request.items_code,
            build=request.build,
            description=request.description,
        )
    except cb.ComponentBuildError as exc:
        raise HTTPException(status_code=400, detail=str(exc))
    return _result_payload(result)


@router.post("/components/rust-alias")
async def author_rust_alias(request: RustLoopAliasRequest):
    """Author a new named loop strategy as a prompt-pattern alias."""
    try:
        result = await asyncio.to_thread(
            cb.author_rust_loop_alias, name=request.name, build=request.build
        )
    except cb.ComponentBuildError as exc:
        raise HTTPException(status_code=400, detail=str(exc))
    return _result_payload(result)


def _result_payload(result: cb.BuildResult) -> dict:
    return {
        "ok": result.ok,
        "name": result.name,
        "kind": result.kind,
        "errors": result.errors,
        "rebuilt": result.rebuilt,
    }
