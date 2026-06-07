//! Project context managers.
//!
//! A [`ContextManager`] discovers project instruction files (and other context)
//! under a workspace root and renders them into a prompt-ready string that the
//! agent loop injects into the system prompt at run time.

use std::path::{Path, PathBuf};

/// A discovered project instruction file.
#[derive(Debug, Clone)]
pub struct ProjectInstructionFile {
    pub path: PathBuf,
    pub content: String,
}

/// Discovers and renders project context for the agent loop.
pub trait ContextManager: Send + Sync {
    /// A stable identifier for this manager.
    fn name(&self) -> &str;

    /// Discover instruction files under `root`.
    fn discover(&self, root: &Path) -> std::io::Result<Vec<ProjectInstructionFile>>;

    /// Render context for `root` within a character budget.
    fn render(&self, root: &Path, budget_chars: usize) -> std::io::Result<String>;
}
