"""Author + build + register new pluggable harness components.

This is the capability that turns the design assistant from a config *filler*
into a component *developer* (Phase 5, Pillar 2). Two authoring paths:

- **Python components** (``coordinator`` / ``scorer``): write a module under
  ``backend/contrib/`` and register it in the component registry. Hot — no build.
- **Rust kernel strategies** (``loop`` / ``compaction``): insert a complete Rust
  item + a registration call into ``src/contrib.rs`` (between its GENERATED
  markers), run ``cargo build`` as the validation gate, and on success catalogue
  the new name in the registry. On build failure the edit is rolled back so the
  tree always compiles.

The cargo invocation is injected (``cargo_runner``) so tests exercise the full
edit / rollback / register logic without a multi-minute compile.
"""

from __future__ import annotations

import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, List, Optional, Tuple

from backend.core.registry import (
    ComponentRegistry,
    get_registry,
    register_contrib_name,
    register_user_component,
)

WORKBENCH_ROOT = Path(__file__).resolve().parents[2]
PY_CONTRIB_DIR = WORKBENCH_ROOT / "backend" / "contrib"
CONTRIB_RS = WORKBENCH_ROOT / "src" / "contrib.rs"

_ITEMS_MARKER = "// <<< GENERATED ITEMS"
_REGS_MARKER = "// <<< GENERATED REGISTRATIONS"

# (returncode, stdout, stderr)
CargoRunner = Callable[[List[str]], Tuple[int, str, str]]


def _default_cargo_runner(args: List[str]) -> Tuple[int, str, str]:
    proc = subprocess.run(
        args, cwd=str(WORKBENCH_ROOT), capture_output=True, text=True
    )
    return proc.returncode, proc.stdout, proc.stderr


class ComponentBuildError(RuntimeError):
    """Raised when authoring / building a component fails."""


@dataclass
class BuildResult:
    ok: bool
    name: str
    kind: str
    errors: List[str]
    rebuilt: bool = False


# --------------------------------------------------------------------------- #
# Python components (hot, no build)
# --------------------------------------------------------------------------- #


def author_python_component(
    *,
    kind: str,
    name: str,
    code: str,
    attr: str = "COMPONENT",
    description: str = "",
    params_schema: Optional[dict] = None,
    registry: Optional[ComponentRegistry] = None,
    contrib_dir: Optional[Path] = None,
) -> BuildResult:
    """Write a Python coordinator/scorer module and register it.

    ``code`` must define a module-level attribute named ``attr`` (default
    ``COMPONENT``) — a class (for coordinators) or a callable (for scorers).
    """
    if kind not in ("coordinator", "scorer"):
        raise ComponentBuildError(f"python components are coordinator/scorer, not '{kind}'")
    registry = registry or get_registry()
    contrib_dir = contrib_dir or PY_CONTRIB_DIR
    contrib_dir.mkdir(parents=True, exist_ok=True)

    safe = "".join(c if c.isalnum() or c == "_" else "_" for c in name).strip("_")
    if not safe:
        raise ComponentBuildError("component name must contain alphanumerics")
    path = contrib_dir / f"{safe}.py"
    path.write_text(code if code.endswith("\n") else code + "\n", encoding="utf-8")

    try:
        register_user_component(
            registry,
            kind=kind,
            name=name,
            load_spec=f"{path}:{attr}",
            description=description,
            params_schema=params_schema,
        )
    except Exception as exc:  # noqa: BLE001 - surface load errors, keep file for debugging
        return BuildResult(ok=False, name=name, kind=kind, errors=[f"load failed: {exc}"])

    return BuildResult(ok=True, name=name, kind=kind, errors=[])


# --------------------------------------------------------------------------- #
# Rust kernel strategies (insert -> cargo build -> register, else rollback)
# --------------------------------------------------------------------------- #


def _insert_before_marker(text: str, marker: str, block: str) -> str:
    if marker not in text:
        raise ComponentBuildError(f"marker not found in src/contrib.rs: {marker}")
    indented = "\n".join(("    " + line if line.strip() else line) for line in block.splitlines())
    return text.replace(marker, f"{indented}\n    {marker}", 1)


def author_rust_strategy(
    *,
    name: str,
    kind: str,
    registration_code: str,
    items_code: str = "",
    build: bool = True,
    cargo_runner: Optional[CargoRunner] = None,
    registry: Optional[ComponentRegistry] = None,
    description: str = "",
    contrib_rs: Optional[Path] = None,
) -> BuildResult:
    """Insert a Rust loop/compaction strategy, build it, and register the name.

    ``registration_code`` is the body inserted into the GENERATED REGISTRATIONS
    region (e.g. a ``contrib::register_loop_strategy("x", || ...);`` call).
    ``items_code`` (optional) is inserted into the GENERATED ITEMS region for a
    full ``struct`` + ``impl`` definition.

    On build failure the file is restored to its prior content and the errors
    are returned (the tree always compiles).
    """
    if kind not in ("loop", "compaction"):
        raise ComponentBuildError(f"rust strategies are loop/compaction, not '{kind}'")
    registry = registry or get_registry()
    contrib_rs = contrib_rs or CONTRIB_RS
    cargo_runner = cargo_runner or _default_cargo_runner

    original = contrib_rs.read_text(encoding="utf-8")
    updated = original
    if items_code.strip():
        updated = _insert_before_marker(updated, _ITEMS_MARKER, items_code)
    updated = _insert_before_marker(updated, _REGS_MARKER, registration_code)
    contrib_rs.write_text(updated, encoding="utf-8")

    rebuilt = False
    if build:
        rc, _out, err = cargo_runner(["cargo", "build", "-p", "agent-workbench"])
        rebuilt = True
        if rc != 0:
            contrib_rs.write_text(original, encoding="utf-8")  # rollback
            tail = "\n".join(err.strip().splitlines()[-40:])
            return BuildResult(ok=False, name=name, kind=kind, errors=[tail or "cargo build failed"], rebuilt=True)

    register_contrib_name(registry, kind=kind, name=name, description=description)
    return BuildResult(ok=True, name=name, kind=kind, errors=[], rebuilt=rebuilt)


def author_rust_loop_alias(
    *,
    name: str,
    build: bool = True,
    cargo_runner: Optional[CargoRunner] = None,
    registry: Optional[ComponentRegistry] = None,
    contrib_rs: Optional[Path] = None,
) -> BuildResult:
    """Convenience: register a new named loop strategy as a prompt-pattern alias.

    The lowest-risk form of authoring a kernel strategy — no new struct, just a
    new selectable ``strategy.type`` name backed by ``PromptPatternStrategy``.
    """
    reg = (
        f'contrib::register_loop_strategy("{name}", || {{\n'
        f'    Arc::new(PromptPatternStrategy::new("{name}"))\n'
        f'}});'
    )
    return author_rust_strategy(
        name=name,
        kind="loop",
        registration_code=reg,
        build=build,
        cargo_runner=cargo_runner,
        registry=registry,
        description=f"prompt-pattern loop alias '{name}'",
        contrib_rs=contrib_rs,
    )
