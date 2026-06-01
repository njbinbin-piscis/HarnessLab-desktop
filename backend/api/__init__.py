"""API router initialization."""

from fastapi import APIRouter
from backend.api.config import router as config_router
from backend.api.experiments import router as experiments_router

api_router = APIRouter(prefix="/api/v1")

# Include sub-routers
api_router.include_router(config_router)
api_router.include_router(experiments_router)

__all__ = ["api_router"]
