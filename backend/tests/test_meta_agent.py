"""Tests for Meta-Agent endpoints."""

import pytest
from fastapi.testclient import TestClient

from backend.main import app


@pytest.fixture
def client():
    return TestClient(app)


class TestMetaAgent:
    def test_generate_config(self, client):
        response = client.post(
            "/api/v1/meta-agent/generate",
            json={
                "goal_description": "Create a Python code reviewer",
                "model_preference": "claude-sonnet-4.5",
                "allowed_tools": ["file_io", "shell"],
                "strategy_type": "react",
            },
        )
        assert response.status_code == 200
        data = response.json()
        assert data["status"] == "success"
        assert "agent:" in data["config_yaml"]
        assert data["validation_status"]["valid"] is True
        assert "CodeReviewer" in data["config_yaml"] or "code" in data["config_yaml"].lower()

    def test_list_templates(self, client):
        response = client.get("/api/v1/meta-agent/templates")
        assert response.status_code == 200
        templates = response.json()
        assert len(templates) >= 3
        assert any(t["id"] == "react_basic" for t in templates)

    def test_refine_config(self, client):
        base = client.post(
            "/api/v1/meta-agent/generate",
            json={
                "goal_description": "Research assistant",
                "model_preference": "gpt-4o",
                "allowed_tools": ["web_search"],
                "strategy_type": "react",
            },
        ).json()

        response = client.post(
            "/api/v1/meta-agent/refine",
            json={
                "config_yaml": base["config_yaml"],
                "feedback": "Also support SQL database queries",
                "updated_tools": ["web_search", "file_io"],
            },
        )
        assert response.status_code == 200
        data = response.json()
        assert "SQL" in data["system_prompt"] or "SQL" in data["config_yaml"]
        assert data["validation_status"]["valid"] is True

    def test_generate_requires_description(self, client):
        response = client.post(
            "/api/v1/meta-agent/generate",
            json={"goal_description": "   "},
        )
        assert response.status_code == 400
