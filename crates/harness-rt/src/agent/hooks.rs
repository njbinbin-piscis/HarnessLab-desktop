//! Lifecycle hooks invoked by the agent loop.

use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;

use super::tool::ToolResult;

/// The decision a `before_tool` hook returns.
#[derive(Debug, Clone)]
pub enum HookDecision {
    /// Proceed with the tool call.
    Continue,
    /// Block the tool call with a reason surfaced to the model.
    Block { reason: String },
}

/// Event passed to tool hooks.
pub struct ToolHookEvent<'a> {
    pub session_id: &'a str,
    pub tool_use_id: &'a str,
    pub tool_name: &'a str,
    pub input: &'a Value,
    pub workspace_root: &'a Path,
}

/// Event passed to context hooks.
pub enum ContextHookEvent<'a> {
    TurnStart {
        session_id: &'a str,
    },
    TurnEnd {
        session_id: &'a str,
    },
    BeforeCompact {
        session_id: &'a str,
        message_count: usize,
    },
    AfterCompact {
        session_id: &'a str,
        message_count: usize,
    },
}

/// Extension surface invoked at fixed points in the agent loop.
///
/// All methods have default no-op implementations so an adapter only overrides
/// what it needs.
#[async_trait]
pub trait AgentHooks: Send + Sync {
    async fn before_tool(&self, _ev: &ToolHookEvent<'_>) -> HookDecision {
        HookDecision::Continue
    }

    async fn after_tool(&self, _ev: &ToolHookEvent<'_>, _result: &ToolResult) {}

    async fn on_context_event(&self, _ev: &ContextHookEvent<'_>) {}
}
