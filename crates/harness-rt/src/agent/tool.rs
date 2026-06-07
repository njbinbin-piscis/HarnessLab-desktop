//! Tool execution context and result types.
//!
//! Re-exports [`Tool`] and [`ToolRegistry`] from [`crate::tools`] so the public
//! path `harness_rt::agent::tool::*` carries everything a caller needs.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

pub use crate::tools::{Tool, ToolRegistry};

/// Runtime settings carried alongside a tool invocation.
#[derive(Debug, Clone, Default)]
pub struct ToolSettings {
    /// Maximum bytes a tool result may contain before truncation (0 = unbounded).
    pub max_result_bytes: usize,
}

/// The context handed to every tool execution.
#[derive(Clone)]
pub struct ToolContext {
    pub session_id: String,
    pub workspace_root: PathBuf,
    pub bypass_permissions: bool,
    pub settings: Arc<ToolSettings>,
    pub max_iterations: Option<usize>,
    pub memory_owner_id: String,
    pub pool_session_id: Option<String>,
    pub tool_use_id: Option<String>,
    pub cancel: Arc<AtomicBool>,
}

/// The outcome of a tool execution.
#[derive(Debug, Clone)]
pub struct ToolResult {
    pub content: String,
    pub is_error: bool,
}

impl ToolResult {
    pub fn ok(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: false,
        }
    }

    pub fn error(content: impl Into<String>) -> Self {
        Self {
            content: content.into(),
            is_error: true,
        }
    }
}
