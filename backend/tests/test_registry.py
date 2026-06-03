"""Tests for the unified pluggable-component registry (Phase 5, Layer A)."""

import textwrap

import pytest

from backend.core.registry import (
    KINDS,
    ComponentRegistry,
    ComponentSpec,
    build_default_registry,
    register_contrib_name,
    register_user_component,
)


def test_default_registry_has_all_kinds():
    reg = build_default_registry()
    grouped = reg.as_dict()
    for kind in KINDS:
        assert kind in grouped
    # Kernel-resolved built-ins are catalogued by their YAML names.
    assert "react" in reg.names("loop")
    assert "summary_based" in reg.names("compaction")
    assert "InMemoryVectorStore" in reg.names("memory")
    assert "ProjectContextManager" in reg.names("context")


def test_coordinators_and_scorers_are_instantiable():
    reg = build_default_registry()
    assert "majority_vote" in reg.names("coordinator")
    coord = reg.instantiate("coordinator", "majority_vote")
    assert getattr(coord, "name", None) == "majority_vote"

    # scorers are catalogued from benchmarks.SCORERS and callable.
    assert "exact_match" in reg.names("scorer")
    scorer = reg.instantiate("scorer", "exact_match")
    assert callable(scorer)


def test_kernel_resolved_not_instantiable():
    reg = build_default_registry()
    with pytest.raises(TypeError):
        reg.instantiate("loop", "react")


def test_spec_to_dict_is_json_safe():
    reg = build_default_registry()
    spec = reg.get("compaction", "summary_based")
    assert spec is not None
    d = spec.to_dict()
    assert d["name"] == "summary_based"
    assert d["instantiable"] is False
    assert "summarizer_prompt" in d["params_schema"]
    assert "factory" not in d  # non-serialisable field dropped


def test_register_contrib_name_shows_as_contrib():
    reg = build_default_registry()
    register_contrib_name(
        reg,
        kind="compaction",
        name="entropy_prune",
        description="prune low-entropy turns",
    )
    spec = reg.get("compaction", "entropy_prune")
    assert spec is not None and spec.source == "contrib"
    with pytest.raises(ValueError):
        register_contrib_name(reg, kind="coordinator", name="x")  # not kernel-resolved


def test_register_user_component_from_file(tmp_path):
    mod = tmp_path / "mycoord.py"
    mod.write_text(
        textwrap.dedent(
            """
            class MyCoordinator:
                name = "my_coord"
                def coordinate(self, ctx):
                    return None
            """
        ),
        encoding="utf-8",
    )
    reg = build_default_registry()
    register_user_component(
        reg,
        kind="coordinator",
        name="my_coord",
        load_spec=f"{mod}:MyCoordinator",
        description="user coordinator",
    )
    spec = reg.get("coordinator", "my_coord")
    assert spec is not None and spec.source == "user"
    inst = reg.instantiate("coordinator", "my_coord")
    assert inst.name == "my_coord"


def test_register_user_component_rejects_kernel_kind(tmp_path):
    reg = build_default_registry()
    with pytest.raises(ValueError):
        register_user_component(
            reg, kind="memory", name="x", load_spec="mod:Attr"
        )


def test_unknown_kind_rejected():
    reg = ComponentRegistry()
    with pytest.raises(ValueError):
        reg.register(ComponentSpec(name="x", kind="bogus"))
