"""Bridge to the Rust agent-workbench CLI for deep config validation."""

import asyncio
import os
import tempfile
from pathlib import Path
from typing import List, Optional, Tuple


WORKBENCH_ROOT = Path(__file__).resolve().parents[2]
DEFAULT_BINARY = WORKBENCH_ROOT / "target" / "debug" / "agent-workbench"


def _resolve_binary() -> Optional[Path]:
    env_path = os.environ.get("AGENT_WORKBENCH_BIN")
    if env_path:
        candidate = Path(env_path)
        if candidate.exists():
            return candidate
    if DEFAULT_BINARY.exists():
        return DEFAULT_BINARY
    return None


async def validate_with_rust(config_yaml: str, project_dir: Optional[str] = None) -> Tuple[bool, List[str]]:
    """Validate YAML by invoking `agent-workbench validate`."""
    binary = _resolve_binary()
    if binary is None:
        return True, []

    with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as handle:
        handle.write(config_yaml)
        config_path = handle.name

    cmd = [
        str(binary),
        "validate",
        "--config",
        config_path,
        "--project",
        project_dir or str(WORKBENCH_ROOT),
    ]

    try:
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=str(WORKBENCH_ROOT),
        )
        stdout, stderr = await proc.communicate()
    finally:
        Path(config_path).unlink(missing_ok=True)

    if proc.returncode == 0:
        return True, []

    message = stderr.decode().strip() or stdout.decode().strip() or "Rust validation failed"
    return False, [message]


async def dry_run_with_rust(
    config_yaml: str,
    project_dir: Optional[str] = None,
) -> Tuple[bool, List[str], List[str]]:
    """Run `agent-workbench run` and return stdout lines as event payloads."""
    binary = _resolve_binary()
    if binary is None:
        return False, ["agent-workbench binary not found; run `cargo build` first"], []

    with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as handle:
        handle.write(config_yaml)
        config_path = handle.name

    cmd = [
        str(binary),
        "run",
        "--config",
        config_path,
        "--project",
        project_dir or str(WORKBENCH_ROOT),
    ]

    try:
        proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=str(WORKBENCH_ROOT),
        )
        stdout, stderr = await proc.communicate()
    finally:
        Path(config_path).unlink(missing_ok=True)

    stdout_text = stdout.decode().strip()
    stderr_text = stderr.decode().strip()
    lines = [line for line in stdout_text.splitlines() if line.strip()]

    if proc.returncode != 0:
        error = stderr_text or stdout_text or "Rust dry-run failed"
        return False, [error], lines

    return True, [], lines
