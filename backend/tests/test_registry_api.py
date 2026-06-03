"""Tests for the /api/v1/registry endpoints (Phase 5, Layer A)."""

import pytest
from fastapi.testclient import TestClient

from backend.main import app


@pytest.fixture
def client():
    return TestClient(app)


def test_list_all_components(client):
    resp = client.get("/api/v1/registry")
    assert resp.status_code == 200
    data = resp.json()
    assert "coordinator" in data["components"]
    assert "loop" in data["components"]
    loop_names = {c["name"] for c in data["components"]["loop"]}
    assert "react" in loop_names


def test_filter_by_kind(client):
    resp = client.get("/api/v1/registry", params={"kind": "compaction"})
    assert resp.status_code == 200
    data = resp.json()
    assert data["kind"] == "compaction"
    names = {c["name"] for c in data["components"]}
    assert {"summary_based", "sliding_window", "vector_retrieval"} <= names


def test_unknown_kind_400(client):
    resp = client.get("/api/v1/registry", params={"kind": "bogus"})
    assert resp.status_code == 400


def test_single_component(client):
    resp = client.get("/api/v1/registry/coordinator/majority_vote")
    assert resp.status_code == 200
    data = resp.json()
    assert data["name"] == "majority_vote"
    assert data["instantiable"] is True


def test_single_component_404(client):
    resp = client.get("/api/v1/registry/loop/nope")
    assert resp.status_code == 404
