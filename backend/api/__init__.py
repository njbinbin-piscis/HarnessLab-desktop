"""API router initialization."""

from fastapi import APIRouter
from backend.api.autoloop import router as autoloop_router
from backend.api.config import router as config_router
from backend.api.datasets import router as datasets_router
from backend.api.design import router as design_router
from backend.api.eval_runs import router as eval_runs_router
from backend.api.experiments import router as experiments_router
from backend.api.meta_agent import router as meta_agent_router
from backend.api.registry import router as registry_router
from backend.api.swarm import router as swarm_router

api_router = APIRouter(prefix="/api/v1")

# Include sub-routers
api_router.include_router(autoloop_router)
api_router.include_router(config_router)
api_router.include_router(datasets_router)
api_router.include_router(design_router)
api_router.include_router(eval_runs_router)
api_router.include_router(experiments_router)
api_router.include_router(meta_agent_router)
api_router.include_router(registry_router)
api_router.include_router(swarm_router)

__all__ = ["api_router"]
