"""Tests for Phase 5 Pillar 2: component authoring + design orchestration."""

import textwrap

import pytest

from backend.core import component_builder as cb
from backend.core.design import extract_yaml, generate_design, read_architect_config
from backend.core.registry import build_default_registry
from backend.models.schemas import MetaAgentGenerateRequest


CONTRIB_RS_TEMPLATE = """\
use std::sync::Arc;
fn register_generated_items() {
    // >>> GENERATED ITEMS (do not edit this marker line)
    // <<< GENERATED ITEMS
}
fn register_generated() {
    // >>> GENERATED REGISTRATIONS (do not edit this marker line)
    // <<< GENERATED REGISTRATIONS
}
"""


# --------------------------------------------------------------------------- #
# Python component authoring
# --------------------------------------------------------------------------- #


def test_author_python_coordinator(tmp_path):
    reg = build_default_registry()
    code = textwrap.dedent(
        """
        class _Coord:
            name = "echo"
            def coordinate(self, ctx):
                return None
        COMPONENT = _Coord
        """
    )
    result = cb.author_python_component(
        kind="coordinator", name="echo", code=code,
        registry=reg, contrib_dir=tmp_path,
    )
    assert result.ok
    assert "echo" in reg.names("coordinator")
    inst = reg.instantiate("coordinator", "echo")
    assert inst.name == "echo"


def test_author_python_rejects_kernel_kind(tmp_path):
    with pytest.raises(cb.ComponentBuildError):
        cb.author_python_component(kind="loop", name="x", code="COMPONENT=1", contrib_dir=tmp_path)


# --------------------------------------------------------------------------- #
# Rust strategy authoring (fake cargo runner; isolated contrib.rs)
# --------------------------------------------------------------------------- #


def _contrib_file(tmp_path):
    p = tmp_path / "contrib.rs"
    p.write_text(CONTRIB_RS_TEMPLATE, encoding="utf-8")
    return p


def test_author_rust_alias_success(tmp_path):
    reg = build_default_registry()
    contrib_rs = _contrib_file(tmp_path)
    calls = []

    def fake_cargo(args):
        calls.append(args)
        return (0, "ok", "")

    result = cb.author_rust_loop_alias(
        name="tree_search", cargo_runner=fake_cargo, registry=reg, contrib_rs=contrib_rs,
    )
    assert result.ok and result.rebuilt
    assert calls and calls[0][:2] == ["cargo", "build"]
    # Registration inserted, name catalogued as contrib.
    text = contrib_rs.read_text()
    assert "tree_search" in text
    spec = reg.get("loop", "tree_search")
    assert spec is not None and spec.source == "contrib"


def test_author_rust_build_failure_rolls_back(tmp_path):
    reg = build_default_registry()
    contrib_rs = _contrib_file(tmp_path)
    original = contrib_rs.read_text()

    def failing_cargo(args):
        return (1, "", "error[E0425]: cannot find value `nope`")

    result = cb.author_rust_strategy(
        name="broken", kind="compaction",
        registration_code='contrib::register_compaction_strategy("broken", || nope());',
        cargo_runner=failing_cargo, registry=reg, contrib_rs=contrib_rs,
    )
    assert not result.ok
    assert any("E0425" in e for e in result.errors)
    # File restored; name NOT registered.
    assert contrib_rs.read_text() == original
    assert reg.get("compaction", "broken") is None


def test_author_rust_no_build_registers(tmp_path):
    reg = build_default_registry()
    contrib_rs = _contrib_file(tmp_path)
    result = cb.author_rust_strategy(
        name="lazy", kind="loop",
        registration_code='contrib::register_loop_strategy("lazy", || todo!());',
        build=False, registry=reg, contrib_rs=contrib_rs,
    )
    assert result.ok and not result.rebuilt
    assert reg.get("loop", "lazy") is not None


# --------------------------------------------------------------------------- #
# Design orchestration
# --------------------------------------------------------------------------- #


def test_extract_yaml_from_fence():
    text = "Here is the config:\n```yaml\nagent:\n  name: X\n```\nDone."
    assert extract_yaml(text).startswith("agent:")


def test_architect_config_readable():
    cfg = read_architect_config()
    assert "Architect" in cfg


@pytest.mark.asyncio
async def test_generate_design_fallback_when_no_engine():
    req = MetaAgentGenerateRequest(goal_description="a code reviewer", strategy_type="react")
    result = await generate_design(req, engine_check=lambda: False)
    assert result["source"] == "template"
    assert "agent:" in result["config_yaml"]


class _FakeSession:
    def __init__(self, text):
        self._text = text

    async def start(self):
        pass

    async def send_prompt(self, text):
        self._prompt = text

    async def events(self):
        yield {"type": "session_ready"}
        yield {"type": "turn_start"}
        for chunk in (self._text[i:i + 20] for i in range(0, len(self._text), 20)):
            yield {"type": "text_delta", "delta": chunk}
        yield {"type": "turn_complete"}

    async def close(self):
        pass


@pytest.mark.asyncio
async def test_generate_design_agent_path():
    yaml_block = (
        "```yaml\n"
        "agent:\n  name: Designed\n  model: claude-sonnet-4.5\n"
        "strategy:\n  type: react\n"
        "```"
    )
    req = MetaAgentGenerateRequest(goal_description="design something", strategy_type="react")
    result = await generate_design(
        req,
        engine_check=lambda: True,
        session_factory=lambda: _FakeSession(f"Sure!\n{yaml_block}"),
    )
    assert result["source"] == "agent"
    assert "name: Designed" in result["config_yaml"]
    assert "validation" in result


@pytest.mark.asyncio
async def test_generate_design_agent_falls_back_on_no_yaml():
    req = MetaAgentGenerateRequest(goal_description="x", strategy_type="react")
    result = await generate_design(
        req,
        engine_check=lambda: True,
        session_factory=lambda: _FakeSession("I cannot help with that."),
    )
    assert result["source"] == "template"
    assert "agent_error" in result
