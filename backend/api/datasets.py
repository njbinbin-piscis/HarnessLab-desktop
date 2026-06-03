"""Validation-set (dataset) management endpoints."""

from typing import Optional

from fastapi import APIRouter, HTTPException

from backend.core.datasets import DatasetError, get_dataset_registry
from backend.models.schemas import DatasetUploadRequest

router = APIRouter(prefix="/datasets", tags=["datasets"])


@router.get("")
async def list_datasets():
    """List all validation sets (built-in smoke sets + uploaded files)."""
    reg = get_dataset_registry()
    return {"datasets": [s.to_dict() for s in reg.list()]}


@router.get("/{dataset_id}")
async def get_dataset(dataset_id: str):
    """Fetch one dataset's metadata."""
    reg = get_dataset_registry()
    try:
        return reg.get_spec(dataset_id).to_dict()
    except DatasetError as exc:
        raise HTTPException(status_code=404, detail=str(exc))


@router.get("/{dataset_id}/tasks")
async def get_dataset_tasks(dataset_id: str, limit: Optional[int] = None, offset: int = 0):
    """Page through a dataset's tasks (prompt + expected answer)."""
    reg = get_dataset_registry()
    try:
        return {"dataset_id": dataset_id, "tasks": reg.get_tasks(dataset_id, limit, offset)}
    except DatasetError as exc:
        raise HTTPException(status_code=404, detail=str(exc))


@router.post("")
async def upload_dataset(request: DatasetUploadRequest):
    """Register a user-provided JSONL validation set."""
    reg = get_dataset_registry()
    try:
        spec = reg.register_jsonl(
            request.id,
            request.content,
            name=request.name,
            scorer=request.scorer,
            description=request.description,
            overwrite=request.overwrite,
        )
    except DatasetError as exc:
        raise HTTPException(status_code=400, detail=str(exc))
    return spec.to_dict()


@router.delete("/{dataset_id}")
async def delete_dataset(dataset_id: str):
    """Delete an uploaded dataset (built-ins cannot be deleted)."""
    reg = get_dataset_registry()
    try:
        removed = reg.delete(dataset_id)
    except DatasetError as exc:
        raise HTTPException(status_code=400, detail=str(exc))
    if not removed:
        raise HTTPException(status_code=404, detail=f"dataset '{dataset_id}' not found")
    return {"deleted": dataset_id}
