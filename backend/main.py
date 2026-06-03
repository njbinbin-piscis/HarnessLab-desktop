"""Agent Workbench Visualization Platform - Main Application."""

from backend.core.env import load_env

load_env()

from fastapi import FastAPI
from fastapi.middleware.cors import CORSMiddleware

from backend.api import api_router

app = FastAPI(
    title="Agent Workbench Visualization Platform",
    description="Real-time visualization and experiment management for AI agent development",
    version="0.1.0"
)

# Configure CORS for frontend access
app.add_middleware(
    CORSMiddleware,
    allow_origins=["*"],  # In production, restrict to specific origins
    allow_credentials=True,
    allow_methods=["*"],
    allow_headers=["*"],
)

# Include API routes
app.include_router(api_router)


@app.get("/")
async def root():
    """Root endpoint with API information."""
    return {
        "name": "Agent Workbench Visualization Platform",
        "version": "0.1.0",
        "docs": "/docs",
        "health": "/health"
    }


@app.get("/health")
async def health_check():
    """Health check endpoint."""
    return {"status": "healthy"}


if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="0.0.0.0", port=8000)
