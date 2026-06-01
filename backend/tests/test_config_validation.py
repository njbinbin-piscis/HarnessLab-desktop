"""Tests for configuration validation endpoint."""

import pytest
from fastapi.testclient import TestClient

from backend.main import app


@pytest.fixture
def client():
    """Create a test client for the FastAPI app."""
    return TestClient(app)


class TestConfigValidation:
    """Test suite for /api/v1/config/validate endpoint."""
    
    def test_valid_minimal_config(self, client):
        """Test validation of a minimal valid config."""
        yaml_config = """
agent:
  name: TestAgent
  model: gpt-4
"""
        response = client.post("/api/v1/config/validate", content=yaml_config)
        assert response.status_code == 200
        data = response.json()
        assert data["valid"] is True
        assert len(data["errors"]) == 0
    
    def test_missing_agent_section(self, client):
        """Test validation fails when agent section is missing."""
        yaml_config = """
plugins:
  enabled: true
"""
        response = client.post("/api/v1/config/validate", content=yaml_config)
        assert response.status_code == 200
        data = response.json()
        assert data["valid"] is False
        assert any("agent" in err for err in data["errors"])
    
    def test_missing_agent_name(self, client):
        """Test validation fails when agent name is missing."""
        yaml_config = """
agent:
  model: gpt-4
"""
        response = client.post("/api/v1/config/validate", content=yaml_config)
        assert response.status_code == 200
        data = response.json()
        assert data["valid"] is False
        assert any("name" in err for err in data["errors"])
    
    def test_invalid_yaml_syntax(self, client):
        """Test validation fails on malformed YAML."""
        yaml_config = """
agent:
  name: TestAgent
  model: gpt-4
  invalid: [unclosed
"""
        response = client.post("/api/v1/config/validate", content=yaml_config)
        assert response.status_code == 200
        data = response.json()
        assert data["valid"] is False
        assert any("YAML" in err for err in data["errors"])
    
    def test_empty_config(self, client):
        """Test validation fails on empty config."""
        response = client.post("/api/v1/config/validate", content="")
        assert response.status_code == 200
        data = response.json()
        assert data["valid"] is False
    
    def test_config_with_plugins(self, client):
        """Test validation of config with plugins section."""
        yaml_config = """
agent:
  name: TestAgent
  model: gpt-4
plugins:
  context_manager: simple
  hooks:
    - on_prompt_build
"""
        response = client.post("/api/v1/config/validate", content=yaml_config)
        assert response.status_code == 200
        data = response.json()
        assert data["valid"] is True
    
    def test_deprecated_strategy_warning(self, client):
        """Test that deprecated strategy field generates warning."""
        yaml_config = """
agent:
  name: TestAgent
  model: gpt-4
  strategy: react
"""
        response = client.post("/api/v1/config/validate", content=yaml_config)
        assert response.status_code == 200
        data = response.json()
        assert data["valid"] is True
        assert any("deprecated" in w.lower() for w in data["warnings"])
    
    def test_temperature_warning(self, client):
        """Test that out-of-range temperature generates warning."""
        yaml_config = """
agent:
  name: TestAgent
  model: gpt-4
  temperature: 2.5
"""
        response = client.post("/api/v1/config/validate", content=yaml_config)
        assert response.status_code == 200
        data = response.json()
        assert data["valid"] is True
        assert any("temperature" in w.lower() for w in data["warnings"])
