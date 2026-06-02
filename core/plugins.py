"""Plugin system for Agent Workbench."""

from typing import Dict, Type
import logging

logger = logging.getLogger(__name__)


class PluginRegistry:
    """Manages the registration and retrieval of plugins."""

    def __init__(self):
        self._plugins: Dict[str, Type] = {}

    def register(self, name: str, plugin_class: Type):
        """Register a plugin class under a given name."""
        if name in self._plugins:
            logger.warning(f"Plugin '{name}' is already registered. Overwriting.")
        self._plugins[name] = plugin_class
        logger.info(f"Registered plugin: {name}")

    def get(self, name: str) -> Type:
        """Retrieve a plugin class by name."""
        if name not in self._plugins:
            raise KeyError(f"Plugin '{name}' not found in registry.")
        return self._plugins[name]

    def list_plugins(self) -> list:
        """List all registered plugin names."""
        return list(self._plugins.keys())
