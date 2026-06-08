//! Neutral tool set + registry.
//!
//! A [`Tool`] is an async, schema-described capability the agent loop can call.
//! [`ToolRegistry`] holds the active tools; [`register_neutral_into`] installs
//! the built-in set, honoring an optional per-tool enable map and the current
//! platform.

mod builtins;

pub use builtins::*;

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::agent::tool::{ToolContext, ToolResult};

/// Platform a tool targets. `Any` tools work everywhere; OS-specific tools are
/// only registered on their matching platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    Any,
    Linux,
    Windows,
    Macos,
}

impl Platform {
    /// A stable lowercase identifier (used by the catalog UI).
    pub fn as_str(&self) -> &'static str {
        match self {
            Platform::Any => "any",
            Platform::Linux => "linux",
            Platform::Windows => "windows",
            Platform::Macos => "macos",
        }
    }

    /// Whether this tool should be registered on the host platform.
    pub fn matches_current(&self) -> bool {
        match self {
            Platform::Any => true,
            Platform::Linux => cfg!(target_os = "linux"),
            Platform::Windows => cfg!(target_os = "windows"),
            Platform::Macos => cfg!(target_os = "macos"),
        }
    }
}

/// An async, schema-described capability.
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    /// JSON-schema for the tool input (OpenAI function `parameters`).
    fn parameters(&self) -> Value;
    fn platform(&self) -> Platform {
        Platform::Any
    }
    async fn execute(&self, input: Value, ctx: &ToolContext) -> ToolResult;
}

/// Holds the active tools for a run.
#[derive(Default)]
pub struct ToolRegistry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a tool.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn names(&self) -> Vec<String> {
        self.tools.keys().cloned().collect()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Produce the schemas advertised to the model.
    pub fn schemas(&self) -> Vec<crate::llm::ToolSchema> {
        self.tools
            .values()
            .map(|t| crate::llm::ToolSchema {
                name: t.name().to_string(),
                description: t.description().to_string(),
                parameters: t.parameters(),
            })
            .collect()
    }
}

/// Configuration for the neutral tool set.
#[derive(Debug, Clone, Default)]
pub struct NeutralToolsConfig {
    /// Optional per-tool enable map. When `Some`, only tools mapped to `true`
    /// are registered; when `None`, all built-ins are registered.
    pub builtin_tool_enabled: Option<HashMap<String, bool>>,
}

/// All built-in neutral tools, regardless of platform/enable filtering.
pub fn all_builtin_tools() -> Vec<Arc<dyn Tool>> {
    vec![
        Arc::new(FileReadTool),
        Arc::new(FileWriteTool),
        Arc::new(FileEditTool),
        Arc::new(FileDiffTool),
        Arc::new(FileListTool),
        Arc::new(FileSearchTool),
        Arc::new(ShellTool),
        Arc::new(CodeRunTool),
        Arc::new(ProcessControlTool),
        Arc::new(WebSearchTool),
        Arc::new(UrlFetcherTool),
        Arc::new(RegisterAlgorithmTool),
        Arc::new(FinishTool),
    ]
}

/// Register the built-in neutral tools into `registry`, honoring the optional
/// per-tool enable map and the host platform.
pub fn register_neutral_into(registry: &mut ToolRegistry, cfg: &NeutralToolsConfig) {
    for tool in all_builtin_tools() {
        if !tool.platform().matches_current() {
            continue;
        }
        if let Some(map) = &cfg.builtin_tool_enabled {
            if !map.get(tool.name()).copied().unwrap_or(false) {
                continue;
            }
        }
        registry.register(tool);
    }
}
