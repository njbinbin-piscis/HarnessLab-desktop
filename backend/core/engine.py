"""Bridge to the Rust `agent-workbench exec` live agent loop.

Spawns the workbench binary, feeds it a YAML config + prompt, and streams the
kernel's `AgentEvent` NDJSON output back as parsed dicts. Credentials are passed
through the environment (PISCIS_PROVIDER / PISCIS_API_KEY / PISCIS_BASE_URL).
"""

import asyncio
import json
import os
import tempfile
from pathlib import Path
from typing import AsyncGenerator, Dict, List, Optional

from backend.core.rust_validator import _resolve_binary, WORKBENCH_ROOT


def engine_available() -> bool:
    """True when the workbench binary and an API key are both present."""
    has_key = bool(
        os.environ.get("PISCIS_API_KEY") or os.environ.get("PISCIS_LLM_API_KEY")
    )
    return _resolve_binary() is not None and has_key


def missing_requirements() -> Optional[str]:
    """Return a human-readable reason the live engine cannot run, or None."""
    if _resolve_binary() is None:
        return "agent-workbench binary not found; run `cargo build --bin agent-workbench`"
    if not (os.environ.get("PISCIS_API_KEY") or os.environ.get("PISCIS_LLM_API_KEY")):
        return "no LLM API key configured; set PISCIS_API_KEY (and PISCIS_PROVIDER / PISCIS_BASE_URL)"
    return None


async def stream_exec(
    config_yaml: str,
    prompt: str,
    workspace_path: Optional[str] = None,
    tools: Optional[List[str]] = None,
) -> AsyncGenerator[Dict, None]:
    """Run a live agent turn and yield parsed NDJSON event dicts.

    Each yielded dict has at least a ``type`` field (kernel snake_case event
    tag, plus the workbench-only ``run_start`` / ``run_complete`` envelopes).

    ``tools`` controls the tool registry for tool-availability ablations:
    ``None`` keeps the full default registry, ``[]`` disables all tools
    (reasoning-only), and a non-empty list is passed as a ``--tools`` whitelist.
    """
    binary = _resolve_binary()
    if binary is None:
        yield {"type": "error", "message": missing_requirements() or "engine unavailable"}
        return

    with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as handle:
        handle.write(config_yaml)
        config_path = handle.name

    workspace = workspace_path or str(WORKBENCH_ROOT)
    cmd = [
        str(binary),
        "exec",
        "--config",
        config_path,
        "--prompt",
        prompt,
        "--workspace",
        workspace,
        "--project",
        workspace,
    ]
    if tools is not None:
        if len(tools) == 0:
            cmd.append("--no-tools")
        else:
            cmd.extend(["--tools", ",".join(tools)])

    proc = await asyncio.create_subprocess_exec(
        *cmd,
        stdout=asyncio.subprocess.PIPE,
        stderr=asyncio.subprocess.PIPE,
        cwd=str(WORKBENCH_ROOT),
        env=os.environ.copy(),
    )

    try:
        assert proc.stdout is not None
        async for raw in proc.stdout:
            line = raw.decode(errors="replace").strip()
            if not line:
                continue
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                # Non-JSON stdout (e.g. tool noise) is surfaced as an observation.
                yield {"type": "observation", "content": line}

        await proc.wait()
        if proc.returncode and proc.returncode != 0:
            stderr = b""
            if proc.stderr is not None:
                stderr = await proc.stderr.read()
            message = stderr.decode(errors="replace").strip() or f"exec exited with code {proc.returncode}"
            yield {"type": "error", "message": message}
    finally:
        Path(config_path).unlink(missing_ok=True)
        if proc.returncode is None:
            try:
                proc.terminate()
            except ProcessLookupError:
                pass


class InteractiveSession:
    """A long-lived `agent-workbench exec` session (Layer 2).

    Unlike :func:`stream_exec` (one prompt, then exit), this spawns the binary
    in *interactive* mode (no ``--prompt``) and speaks the NDJSON control
    protocol on the child's stdin:

    - ``prompt``     run a turn (conversation history is retained in the child)
    - ``inject``     steer the *active* turn mid-run (reuses notification_rx)
    - ``set_model``  acknowledged (model is fixed per process for now)
    - ``stop``       end the session

    Events (kernel ``AgentEvent``s + workbench envelopes such as
    ``session_ready`` / ``turn_start`` / ``turn_complete`` / ``injected``) are
    read via :meth:`events`. Typical use spins :meth:`events` into a task while
    driving the session with :meth:`send_prompt` / :meth:`inject`.
    """

    def __init__(
        self,
        config_yaml: str,
        workspace_path: Optional[str] = None,
        tools: Optional[List[str]] = None,
    ) -> None:
        self._config_yaml = config_yaml
        self._workspace = workspace_path or str(WORKBENCH_ROOT)
        self._tools = tools
        self._proc: Optional[asyncio.subprocess.Process] = None
        self._config_path: Optional[str] = None

    async def start(self) -> None:
        """Spawn the interactive child process. Raises if unavailable."""
        binary = _resolve_binary()
        if binary is None:
            raise RuntimeError(missing_requirements() or "engine unavailable")

        with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as handle:
            handle.write(self._config_yaml)
            self._config_path = handle.name

        cmd = [
            str(binary),
            "exec",
            "--config",
            self._config_path,
            "--workspace",
            self._workspace,
            "--project",
            self._workspace,
        ]
        if self._tools is not None:
            if len(self._tools) == 0:
                cmd.append("--no-tools")
            else:
                cmd.extend(["--tools", ",".join(self._tools)])

        self._proc = await asyncio.create_subprocess_exec(
            *cmd,
            stdin=asyncio.subprocess.PIPE,
            stdout=asyncio.subprocess.PIPE,
            stderr=asyncio.subprocess.PIPE,
            cwd=str(WORKBENCH_ROOT),
            env=os.environ.copy(),
        )

    @property
    def running(self) -> bool:
        return self._proc is not None and self._proc.returncode is None

    async def _write(self, obj: Dict) -> None:
        if self._proc is None or self._proc.stdin is None:
            raise RuntimeError("session not started")
        self._proc.stdin.write((json.dumps(obj) + "\n").encode())
        await self._proc.stdin.drain()

    async def send_prompt(self, text: str) -> None:
        """Queue a new turn with the given user prompt."""
        await self._write({"cmd": "prompt", "text": text})

    async def inject(self, text: str) -> None:
        """Steer the active turn (delivered as a mid-run user message)."""
        await self._write({"cmd": "inject", "text": text})

    async def set_model(self, model: str) -> None:
        """Request a model switch (currently acknowledged but not hot-swapped)."""
        await self._write({"cmd": "set_model", "model": model})

    async def stop(self) -> None:
        """Ask the child to end its session loop."""
        try:
            await self._write({"cmd": "stop"})
        except RuntimeError:
            pass

    async def events(self) -> AsyncGenerator[Dict, None]:
        """Yield parsed NDJSON events until the child closes stdout."""
        if self._proc is None or self._proc.stdout is None:
            raise RuntimeError("session not started")
        async for raw in self._proc.stdout:
            line = raw.decode(errors="replace").strip()
            if not line:
                continue
            try:
                yield json.loads(line)
            except json.JSONDecodeError:
                yield {"type": "observation", "content": line}

    async def close(self) -> None:
        """Stop the session and clean up the temp config + process."""
        proc = self._proc
        if proc is not None:
            try:
                await self.stop()
            except Exception:
                pass
            if proc.stdin is not None:
                try:
                    proc.stdin.close()
                except Exception:
                    pass
            try:
                await asyncio.wait_for(proc.wait(), timeout=5.0)
            except asyncio.TimeoutError:
                try:
                    proc.terminate()
                except ProcessLookupError:
                    pass
        if self._config_path:
            Path(self._config_path).unlink(missing_ok=True)
            self._config_path = None

    async def __aenter__(self) -> "InteractiveSession":
        await self.start()
        return self

    async def __aexit__(self, *_exc) -> None:
        await self.close()
