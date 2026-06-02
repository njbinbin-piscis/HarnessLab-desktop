"""Tests for the Plugin Registry system."""

import pytest
from core.plugins import PluginRegistry


class MockPlugin:
    def __init__(self, name):
        self.name = name


def test_register_and_get_plugin():
    registry = PluginRegistry()
    registry.register("mock", MockPlugin)
    
    plugin_class = registry.get("mock")
    assert plugin_class == MockPlugin
    
    instance = plugin_class("test_instance")
    assert instance.name == "test_instance"


def test_list_plugins():
    registry = PluginRegistry()
    registry.register("plugin_a", MockPlugin)
    registry.register("plugin_b", MockPlugin)
    
    plugins = registry.list_plugins()
    assert "plugin_a" in plugins
    assert "plugin_b" in plugins
    assert len(plugins) == 2


def test_get_non_existent_plugin():
    registry = PluginRegistry()
    with pytest.raises(KeyError):
        registry.get("non_existent")


def test_overwrite_plugin():
    registry = PluginRegistry()
    registry.register("mock", MockPlugin)
    
    class AnotherMock:
        pass
        
    registry.register("mock", AnotherMock)
    assert registry.get("mock") == AnotherMock
