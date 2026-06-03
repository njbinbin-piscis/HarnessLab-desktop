"""Unified pluggable-component registry (Phase 5, Layer A foundation).

Every part of the platform that can be swapped — multi-agent coordinators,
benchmark scorers, and the kernel-resolved memory / context / loop / compaction
strategies — is catalogued here behind one schema. The registry is what makes
the platform *pluggable* and *tunable*:

- **Pluggable**: a component is addressed by ``(kind, name)``; new components are
  registered (built-in, kernel ``contrib``, or user-loaded) without touching
  callers. The design assistant (Phase D) registers the components it authors
  here so they immediately become selectable.
- **Tunable**: every entry carries a ``params_schema`` (a small JSON-Schema-ish
  dict) describing the knobs the component exposes, so the config layer / UI can
  render parameter forms and sweeps.

Two flavours of entry:

- **Python-instantiable** (``coordinator``, ``scorer``): carry a ``factory`` and
  can be constructed in-process.
- **Kernel-resolved** (``memory``, ``context``, ``loop``, ``compaction``):
  descriptive catalogue entries whose ``name`` is what a YAML config passes to
  the Rust loader / kernel resolvers. ``factory`` is ``None``; ``source`` tells
  you whether it is a built-in or a kernel ``contrib`` extension.
"""

from __future__ import annotations

import importlib
import importlib.util
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Callable, Dict, List, Optional, Tuple

# Kinds the registry understands. Keep in sync with the kernel resolvers and the
# YAML schema in ``src/config.rs``.
KINDS = ("coordinator", "scorer", "memory", "context", "loop", "compaction")

Source = str  # "builtin" | "contrib" | "user"


@dataclass
class ComponentSpec:
    """One catalogued pluggable component."""

    name: str
    kind: str
    description: str = ""
    # JSON-Schema-ish: {"<param>": {"type": ..., "default": ..., "description": ...}}
    params_schema: Dict[str, Any] = field(default_factory=dict)
    source: Source = "builtin"
    # Python-instantiable kinds carry a factory; kernel-resolved kinds leave it None.
    factory: Optional[Callable[..., Any]] = None
    # For user-loaded entries: the load spec ("module:Attr" / "file.py:Attr").
    load_spec: Optional[str] = None

    def to_dict(self) -> Dict[str, Any]:
        """JSON-safe view (drops the non-serialisable factory)."""
        return {
            "name": self.name,
            "kind": self.kind,
            "description": self.description,
            "params_schema": self.params_schema,
            "source": self.source,
            "instantiable": self.factory is not None,
            "load_spec": self.load_spec,
        }


class ComponentRegistry:
    """In-process catalogue of pluggable components keyed by ``(kind, name)``."""

    def __init__(self) -> None:
        self._items: Dict[Tuple[str, str], ComponentSpec] = {}

    def register(self, spec: ComponentSpec, *, overwrite: bool = True) -> ComponentSpec:
        if spec.kind not in KINDS:
            raise ValueError(f"unknown component kind '{spec.kind}'. Known: {', '.join(KINDS)}")
        key = (spec.kind, spec.name)
        if key in self._items and not overwrite:
            raise ValueError(f"component already registered: {spec.kind}/{spec.name}")
        self._items[key] = spec
        return spec

    def get(self, kind: str, name: str) -> Optional[ComponentSpec]:
        return self._items.get((kind, name))

    def list(self, kind: Optional[str] = None) -> List[ComponentSpec]:
        items = [s for (k, _), s in self._items.items() if kind is None or k == kind]
        return sorted(items, key=lambda s: (s.kind, s.name))

    def names(self, kind: str) -> List[str]:
        return sorted(name for (k, name) in self._items if k == kind)

    def instantiate(self, kind: str, name: str, **params: Any) -> Any:
        """Build a Python-instantiable component (coordinator / scorer)."""
        spec = self.get(kind, name)
        if spec is None:
            raise KeyError(f"no such component: {kind}/{name}")
        if spec.factory is None:
            raise TypeError(
                f"component {kind}/{name} is kernel-resolved (source={spec.source}); "
                "pass its name to the YAML config rather than instantiating it in Python"
            )
        return spec.factory(**params) if params else spec.factory()

    def as_dict(self) -> Dict[str, List[Dict[str, Any]]]:
        """Grouped JSON-safe snapshot: {kind: [spec, ...]}."""
        grouped: Dict[str, List[Dict[str, Any]]] = {k: [] for k in KINDS}
        for spec in self.list():
            grouped[spec.kind].append(spec.to_dict())
        return grouped


# --------------------------------------------------------------------------- #
# Built-in population
# --------------------------------------------------------------------------- #

# Kernel-resolved names. These MUST mirror the Rust resolvers:
#   - memory/context: agent-workbench/src/loader.rs
#   - loop:           piscis-kernel/src/agent/loop_strategy.rs::resolve_loop_strategy
#   - compaction:     piscis-kernel/src/agent/loop_.rs::resolve_compaction_strategy
_KERNEL_BUILTINS: Dict[str, List[Dict[str, Any]]] = {
    "memory": [
        {"name": "NoOpMemory", "description": "Ephemeral; no persistence (fish/short tasks)."},
        {"name": "InMemoryVectorStore", "description": "Session-scoped in-memory vector store."},
        {"name": "SqliteMemory", "description": "Persistent SQLite memory (WIP; falls back to in-memory)."},
    ],
    "context": [
        {"name": "ProjectContextManager", "description": "Scan ancestor dirs for instruction files (PISCI.md, ...)."},
        {"name": "WorkspaceRootManager", "description": "Shallow scan of README.md / PISCI.md at the root."},
        {"name": "DynamicContextManager", "description": "Semantic-search context stub."},
    ],
    "loop": [
        {"name": "react", "description": "Built-in ReAct loop (default)."},
        {"name": "plan_and_solve", "description": "Plan-then-execute prompt pattern."},
        {"name": "reflexion", "description": "Reflective-retry prompt pattern."},
    ],
    "compaction": [
        {"name": "summary_based", "description": "LLM rolling-summary compaction (default).",
         "params_schema": {"summarizer_prompt": {"type": "string", "default": None,
                                                 "description": "Optional summary prompt override."}}},
        {"name": "sliding_window", "description": "Token-budget truncation, no LLM call."},
        {"name": "vector_retrieval", "description": "Relevance-ranked retention of older messages."},
    ],
}


def _register_kernel_builtins(reg: ComponentRegistry) -> None:
    for kind, entries in _KERNEL_BUILTINS.items():
        for entry in entries:
            reg.register(
                ComponentSpec(
                    name=entry["name"],
                    kind=kind,
                    description=entry.get("description", ""),
                    params_schema=entry.get("params_schema", {}),
                    source="builtin",
                )
            )


def _register_coordinators(reg: ComponentRegistry) -> None:
    # Lazy import to avoid a hard cycle at module import time.
    from backend.core import swarm

    for name, cls in swarm._BUILTINS.items():  # noqa: SLF001 - intentional catalogue read
        reg.register(
            ComponentSpec(
                name=name,
                kind="coordinator",
                description=(cls.__doc__ or "").strip().splitlines()[0] if cls.__doc__ else "",
                source="builtin",
                factory=cls,
            )
        )


def _register_scorers(reg: ComponentRegistry) -> None:
    try:
        from benchmarks.scorers import SCORERS
    except Exception:  # benchmarks optional at import time
        return
    descriptions = {
        "exact_match": "Normalized exact-string match (GAIA / closed-form QA).",
        "f1_match": "Token-level F1, success at F1>=0.5 (HotpotQA).",
        "numeric_match": "Last-number match (GSM8K).",
        "fail_to_pass": "External test-harness resolved flag (SWE-bench).",
    }
    for name, fn in SCORERS.items():
        reg.register(
            ComponentSpec(
                name=name,
                kind="scorer",
                description=descriptions.get(name, (fn.__doc__ or "").strip().splitlines()[0] if fn.__doc__ else ""),
                source="builtin",
                factory=lambda _fn=fn: _fn,
            )
        )


def build_default_registry() -> ComponentRegistry:
    """Construct a registry pre-populated with all built-in components."""
    reg = ComponentRegistry()
    _register_kernel_builtins(reg)
    _register_coordinators(reg)
    _register_scorers(reg)
    return reg


# --------------------------------------------------------------------------- #
# User / contrib loading
# --------------------------------------------------------------------------- #


def _load_attr(spec: str) -> Any:
    """Resolve a ``module:Attr`` or ``/path/file.py:Attr`` reference."""
    if ":" not in spec:
        raise ValueError(f"load spec must be 'module:Attr' or 'file.py:Attr', got '{spec}'")
    target, attr = spec.rsplit(":", 1)
    if target.endswith(".py") or "/" in target:
        path = Path(target).expanduser().resolve()
        if not path.exists():
            raise FileNotFoundError(f"component module not found: {path}")
        module_spec = importlib.util.spec_from_file_location(f"_registry_user_{path.stem}", path)
        if module_spec is None or module_spec.loader is None:
            raise ImportError(f"cannot load component module: {path}")
        module = importlib.util.module_from_spec(module_spec)
        module_spec.loader.exec_module(module)
    else:
        module = importlib.import_module(target)
    if not hasattr(module, attr):
        raise AttributeError(f"module '{target}' has no attribute '{attr}'")
    return getattr(module, attr)


def register_user_component(
    reg: ComponentRegistry,
    *,
    kind: str,
    name: str,
    load_spec: str,
    description: str = "",
    params_schema: Optional[Dict[str, Any]] = None,
) -> ComponentSpec:
    """Load a user-authored Python component and register it as ``source='user'``.

    Only the Python-instantiable kinds (``coordinator``, ``scorer``) are loaded
    here; kernel-resolved kinds (memory/context/loop/compaction) are registered
    via :func:`register_contrib_name` once their Rust side is built.
    """
    if kind not in ("coordinator", "scorer"):
        raise ValueError(
            f"kind '{kind}' is kernel-resolved; use register_contrib_name() instead"
        )
    obj = _load_attr(load_spec)
    factory = obj if callable(obj) else (lambda _o=obj: _o)
    return reg.register(
        ComponentSpec(
            name=name,
            kind=kind,
            description=description,
            params_schema=params_schema or {},
            source="user",
            factory=factory,
            load_spec=load_spec,
        )
    )


def register_contrib_name(
    reg: ComponentRegistry,
    *,
    kind: str,
    name: str,
    description: str = "",
    params_schema: Optional[Dict[str, Any]] = None,
) -> ComponentSpec:
    """Catalogue a kernel ``contrib`` component (built in Rust, resolved by name).

    The design assistant calls this after authoring + building a new kernel
    strategy so it shows up as a selectable option even though there is no
    Python factory for it.
    """
    if kind not in ("memory", "context", "loop", "compaction"):
        raise ValueError(f"kind '{kind}' is not kernel-resolved")
    return reg.register(
        ComponentSpec(
            name=name,
            kind=kind,
            description=description,
            params_schema=params_schema or {},
            source="contrib",
        )
    )


# Global default registry (mutable; the design assistant appends to it).
_global_registry: Optional[ComponentRegistry] = None


def get_registry() -> ComponentRegistry:
    global _global_registry
    if _global_registry is None:
        _global_registry = build_default_registry()
    return _global_registry
