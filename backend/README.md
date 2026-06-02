# Agent Workbench Visualization Platform - Backend

FastAPI-based backend for the Agent Workbench Visualization Platform. Provides REST API endpoints for experiment management, configuration validation, and real-time SSE streaming of agent execution events.

## Architecture

```
backend/
├── __init__.py
├── main.py              # FastAPI application entry point
├── requirements.txt     # Python dependencies
├── api/
│   ├── __init__.py      # API router aggregation
│   ├── config.py        # Configuration validation endpoint
│   └── experiments.py   # Experiment lifecycle endpoints
├── core/
│   ├── __init__.py
│   ├── manager.py       # ExperimentManager - lifecycle management
│   └── streaming.py     # EventStream - SSE streaming support
├── models/
│   ├── __init__.py
│   └── schemas.py       # Pydantic data models
└── tests/
    ├── __init__.py
    ├── test_config_validation.py
    └── test_experiment_manager.py
```

## API Endpoints

### Configuration Validation
- `POST /api/v1/config/validate` - Validate YAML configuration without running

### Experiment Management
- `POST /api/v1/experiments` - Start a new experiment
- `GET /api/v1/experiments/{id}/stream` - SSE stream of agent events
- `GET /api/v1/experiments/{id}/history` - Get full execution history
- `GET /api/v1/experiments/{id}/state` - Get current experiment state

## SSE Event Types

| Event | Description |
|-------|-------------|
| `thought` | Agent's internal reasoning |
| `action` | Tool call being prepared |
| `observation` | Tool execution result |
| `error` | Execution error |
| `complete` | Experiment finished |

## Quick Start

```bash
# Install dependencies
pip install -r requirements.txt

# Run the server
python -m backend.main

# Or with uvicorn directly
uvicorn backend.main:app --reload --host 0.0.0.0 --port 8000
```

## Testing

```bash
pytest backend/tests/ -v
```

## Integration with piscis-engine

The backend is designed to integrate with the Rust-based `piscis-engine` core. In production:
1. The `ExperimentManager` spawns the engine as a subprocess or via PyO3 bindings
2. Engine output is parsed into `AgentEvent` objects
3. Events are pushed to the SSE stream for real-time frontend visualization
