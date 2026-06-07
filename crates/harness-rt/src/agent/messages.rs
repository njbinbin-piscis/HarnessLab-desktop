//! Events streamed from the agent loop to the host.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// A single plan item (for `PlanUpdate`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanItem {
    pub text: String,
    pub status: String,
}

/// Events emitted by [`AgentLoop::run`](crate::agent::loop_::AgentLoop::run).
///
/// Serialized with a `type` tag (snake_case) so a host can forward them as
/// NDJSON / over IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    /// A new assistant text segment is starting at `iteration`.
    TextSegmentStart {
        iteration: u32,
    },
    /// Incremental assistant text.
    TextDelta {
        delta: String,
    },
    /// A tool call is starting.
    ToolStart {
        id: String,
        name: String,
        input: Value,
    },
    /// A tool call finished.
    ToolEnd {
        id: String,
        name: String,
        result: String,
        is_error: bool,
    },
    /// The run completed successfully with cumulative token counts.
    Done {
        total_input_tokens: u32,
        total_output_tokens: u32,
    },
    /// The run was cancelled.
    Cancelled,
    /// The run failed.
    Error {
        message: String,
    },
    /// The agent's plan changed.
    PlanUpdate {
        items: Vec<PlanItem>,
    },
    /// Context-window usage telemetry.
    ContextUsage {
        estimated_input_tokens: u32,
        cumulative_input_tokens: u32,
        cumulative_output_tokens: u32,
        context_window: u32,
    },
    /// A message was committed to history (reserved).
    MessageCommit {
        role: String,
    },
    /// A permission request (reserved; the desktop runs with bypass).
    PermissionRequest {
        tool: String,
    },
    /// Interactive UI payloads (reserved).
    InteractiveUi {
        payload: Value,
    },
    InteractiveUiPatch {
        payload: Value,
    },
    InteractiveUiListen {
        id: String,
    },
    /// Long-running progress (reserved).
    FishProgress {
        message: String,
    },
}
