"""Load agent-workbench environment variables from .env if present."""

from __future__ import annotations

from pathlib import Path

WORKBENCH_ROOT = Path(__file__).resolve().parents[2]
_ENV_LOADED = False


def load_env() -> None:
    """Load ``.env`` from the workbench root once per process."""
    global _ENV_LOADED
    if _ENV_LOADED:
        return
    try:
        from dotenv import load_dotenv
    except ImportError:
        _ENV_LOADED = True
        return
    load_dotenv(WORKBENCH_ROOT / ".env", override=False)
    _ENV_LOADED = True
