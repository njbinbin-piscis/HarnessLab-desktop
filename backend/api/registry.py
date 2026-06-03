"""Component registry endpoints — the catalogue of pluggable parts.

Exposes every swappable component (coordinators, scorers, and the
kernel-resolved memory / context / loop / compaction strategies) with its
parameter schema, so the UI and the design assistant can discover what is
available and how to tune it.
"""

from typing import Optional

from fastapi import APIRouter, HTTPException

from backend.core.registry import KINDS, get_registry

router = APIRouter(prefix="/registry", tags=["registry"])


@router.get("")
async def get_components(kind: Optional[str] = None):
    """List pluggable components, optionally filtered by ``kind``.

    Returns a grouped catalogue ``{kind: [spec, ...]}`` when no filter is given,
    or a flat list for a single kind.
    """
    reg = get_registry()
    if kind is None:
        return {"kinds": list(KINDS), "components": reg.as_dict()}
    if kind not in KINDS:
        raise HTTPException(
            status_code=400,
            detail=f"unknown kind '{kind}'. Known: {', '.join(KINDS)}",
        )
    return {"kind": kind, "components": [s.to_dict() for s in reg.list(kind)]}


@router.get("/{kind}/{name}")
async def get_component(kind: str, name: str):
    """Fetch a single component spec (name, source, params schema)."""
    reg = get_registry()
    spec = reg.get(kind, name)
    if spec is None:
        raise HTTPException(status_code=404, detail=f"no such component: {kind}/{name}")
    return spec.to_dict()
