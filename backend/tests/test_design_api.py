"""API tests for the /api/v1/design endpoints (Pillar 2)."""

import textwrap

import pytest
from fastapi.testclient import TestClient

from backend.main import app


@pytest.fixture
def client():
    return TestClient(app)


def test_get_architect(client):
    resp = client.get("/api/v1/design/architect")
    assert resp.status_code == 200
    assert "Architect" in resp.json()["config_yaml"]


def test_design_generate_fallback(client, monkeypatch):
    # Force the template path so no engine is needed.
    import backend.core.design as design

    monkeypatch.setattr(design, "engine_available", lambda: False)
    resp = client.post("/api/v1/design/generate", json={"goal_description": "a data analyst"})
    assert resp.status_code == 200
    data = resp.json()
    assert data["source"] == "template"
    assert "agent:" in data["config_yaml"]


def test_design_generate_requires_goal(client):
    resp = client.post("/api/v1/design/generate", json={"goal_description": "   "})
    assert resp.status_code == 400


def test_author_python_component_endpoint(client, tmp_path, monkeypatch):
    # Redirect the contrib dir so the test does not pollute the repo.
    import backend.core.component_builder as cbmod

    monkeypatch.setattr(cbmod, "PY_CONTRIB_DIR", tmp_path)
    code = textwrap.dedent(
        """
        class _S:
            name = "len_scorer"
            def __call__(self, task, text):
                return (bool(text), float(len(text) > 0))
        COMPONENT = _S
        """
    )
    resp = client.post("/api/v1/design/components/python", json={
        "kind": "scorer", "name": "len_scorer", "code": code,
    })
    assert resp.status_code == 200
    body = resp.json()
    assert body["ok"] is True
    assert body["kind"] == "scorer"


def test_author_rust_alias_no_build(client, tmp_path, monkeypatch):
    # Redirect contrib.rs to an isolated temp file (with markers) and skip the
    # cargo build so the test neither mutates the repo nor compiles.
    import backend.core.component_builder as cbmod

    contrib = tmp_path / "contrib.rs"
    contrib.write_text(
        "fn register_generated() {\n"
        "    // >>> GENERATED REGISTRATIONS (do not edit this marker line)\n"
        "    // <<< GENERATED REGISTRATIONS\n"
        "}\n",
        encoding="utf-8",
    )
    monkeypatch.setattr(cbmod, "CONTRIB_RS", contrib)

    resp = client.post("/api/v1/design/components/rust-alias", json={
        "name": "zzz_test_alias_api", "build": False,
    })
    assert resp.status_code == 200
    assert resp.json()["ok"] is True
    assert "zzz_test_alias_api" in contrib.read_text()
