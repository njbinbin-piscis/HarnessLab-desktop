//! Timing hook — migrated to `harness_rt::agent::hooks`.
//!
//! Demonstrates and tests the native runtime hook surface: `before_tool` /
//! `after_tool` record per-tool latency; `on_context_event` observes turn
//! boundaries (replacing the old kernel's `on_llm_call_event`).

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use harness_rt::agent::hooks::{AgentHooks, ContextHookEvent, HookDecision, ToolHookEvent};
use harness_rt::agent::tool::ToolResult;
use serde_json::json;

/// Records tool execution latency keyed by `tool_use_id`.
pub struct TimingHook {
    tool_starts: Mutex<HashMap<String, Instant>>,
    turn_starts: Mutex<HashMap<String, Instant>>,
}

impl TimingHook {
    pub fn new() -> Self {
        Self {
            tool_starts: Mutex::new(HashMap::new()),
            turn_starts: Mutex::new(HashMap::new()),
        }
    }

    /// Elapsed time recorded for a completed tool call (test helper).
    pub fn tool_elapsed(&self, tool_use_id: &str) -> Option<Duration> {
        let starts = self.tool_starts.lock().unwrap();
        starts.get(tool_use_id).map(|t| t.elapsed())
    }
}

impl Default for TimingHook {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AgentHooks for TimingHook {
    async fn before_tool(&self, ev: &ToolHookEvent<'_>) -> HookDecision {
        self.tool_starts
            .lock()
            .unwrap()
            .insert(ev.tool_use_id.to_string(), Instant::now());
        HookDecision::Continue
    }

    async fn after_tool(&self, ev: &ToolHookEvent<'_>, result: &ToolResult) {
        let mut map = self.tool_starts.lock().unwrap();
        if let Some(start) = map.remove(ev.tool_use_id) {
            eprintln!(
                "[TimingHook] tool={} id={} elapsed={:?} error={}",
                ev.tool_name,
                ev.tool_use_id,
                start.elapsed(),
                result.is_error
            );
        }
    }

    async fn on_context_event(&self, ev: &ContextHookEvent<'_>) {
        match ev {
            ContextHookEvent::TurnStart { session_id } => {
                self.turn_starts
                    .lock()
                    .unwrap()
                    .insert(session_id.to_string(), Instant::now());
                eprintln!("[TimingHook] turn_start session={session_id}");
            }
            ContextHookEvent::TurnEnd { session_id } => {
                let mut map = self.turn_starts.lock().unwrap();
                if let Some(start) = map.remove(*session_id) {
                    eprintln!(
                        "[TimingHook] turn_end session={session_id} elapsed={:?}",
                        start.elapsed()
                    );
                }
            }
            ContextHookEvent::BeforeCompact {
                session_id,
                message_count,
            } => {
                eprintln!(
                    "[TimingHook] before_compact session={session_id} messages={message_count}"
                );
            }
            ContextHookEvent::AfterCompact {
                session_id,
                message_count,
            } => {
                eprintln!(
                    "[TimingHook] after_compact session={session_id} messages={message_count}"
                );
            }
        }
    }
}

#[tokio::test]
async fn before_tool_records_start_and_returns_continue() {
    let hook = TimingHook::new();
    let input = json!({ "path": "README.md" });
    let ev = ToolHookEvent {
        session_id: "sess-1",
        tool_use_id: "tu-1",
        tool_name: "file_read",
        input: &input,
        workspace_root: Path::new("/tmp/ws"),
    };

    let decision = hook.before_tool(&ev).await;
    assert!(matches!(decision, HookDecision::Continue));
    assert!(hook.tool_starts.lock().unwrap().contains_key("tu-1"));
}

#[tokio::test]
async fn after_tool_clears_start_entry() {
    let hook = TimingHook::new();
    let input = json!({ "cmd": "echo hi" });
    let ev = ToolHookEvent {
        session_id: "sess-1",
        tool_use_id: "tu-2",
        tool_name: "shell",
        input: &input,
        workspace_root: Path::new("/tmp/ws"),
    };

    hook.before_tool(&ev).await;
    std::thread::sleep(Duration::from_millis(5));
    hook.after_tool(&ev, &ToolResult::ok("done")).await;

    assert!(!hook.tool_starts.lock().unwrap().contains_key("tu-2"));
}

#[tokio::test]
async fn context_turn_events_track_session() {
    let hook = TimingHook::new();

    hook.on_context_event(&ContextHookEvent::TurnStart {
        session_id: "sess-9",
    })
    .await;
    assert!(hook.turn_starts.lock().unwrap().contains_key("sess-9"));

    hook.on_context_event(&ContextHookEvent::TurnEnd {
        session_id: "sess-9",
    })
    .await;
    assert!(!hook.turn_starts.lock().unwrap().contains_key("sess-9"));
}
