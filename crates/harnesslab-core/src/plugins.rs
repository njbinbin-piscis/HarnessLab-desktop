//! Plugin system for Agent Workbench.
//!
//! Defines traits for core extensible components: ContextManager, Memory, and Hooks.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// A discovered context entry from the project workspace.
#[derive(Debug, Clone)]
pub struct ContextEntry {
    pub path: PathBuf,
    pub content: String,
    pub priority: u8, // Higher priority entries are placed closer to the system prompt
}

/// Trait for discovering and rendering project-specific context.
pub trait ContextManager: Send + Sync {
    /// Discover and load context from the given root directory.
    fn discover(&self, root: &Path) -> Result<Vec<ContextEntry>>;

    /// Render discovered context into a prompt-ready string.
    fn render(&self, entries: &[ContextEntry]) -> String;
}

/// Trait for agent memory management.
pub trait Memory: Send + Sync {
    /// Add a message or observation to memory.
    fn add(&mut self, content: &str);

    /// Retrieve relevant memories based on a query.
    fn retrieve(&self, query: &str, limit: usize) -> Vec<String>;

    /// Clear all memory.
    fn clear(&mut self);
}

/// Trait for lifecycle hooks.
pub trait Hook: Send + Sync {
    /// Called before a tool is executed.
    fn before_tool(&self, tool_name: &str, args: &serde_json::Value) -> Result<()>;

    /// Called after a tool is executed.
    fn after_tool(&self, tool_name: &str, result: &serde_json::Value) -> Result<()>;

    /// Called during prompt assembly.
    fn on_prompt_assemble(&self, prompt: &mut String) -> Result<()>;
}

/// No-op implementation of ContextManager for testing or minimal setups.
pub struct NoOpContextManager;

impl ContextManager for NoOpContextManager {
    fn discover(&self, _root: &Path) -> Result<Vec<ContextEntry>> {
        Ok(Vec::new())
    }

    fn render(&self, _entries: &[ContextEntry]) -> String {
        String::new()
    }
}

/// No-op implementation of Memory.
pub struct NoOpMemory;

impl Memory for NoOpMemory {
    fn add(&mut self, _content: &str) {}
    fn retrieve(&self, _query: &str, _limit: usize) -> Vec<String> {
        Vec::new()
    }
    fn clear(&mut self) {}
}

/// No-op implementation of Hook.
pub struct NoOpHook;

impl Hook for NoOpHook {
    fn before_tool(&self, _tool_name: &str, _args: &serde_json::Value) -> Result<()> {
        Ok(())
    }
    fn after_tool(&self, _tool_name: &str, _result: &serde_json::Value) -> Result<()> {
        Ok(())
    }
    fn on_prompt_assemble(&self, _prompt: &mut String) -> Result<()> {
        Ok(())
    }
}
