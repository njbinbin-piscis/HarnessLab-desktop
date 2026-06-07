//! Evolution Hook implementation for Agent Workbench.
//!
//! Tracks performance metrics and triggers configuration refinement proposals.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use async_trait::async_trait;
use piscis_kernel::agent::hooks::{AgentHooks, ContextHookEvent, HookDecision, ToolHookEvent};
use piscis_kernel::agent::tool::ToolResult;

/// Performance metrics tracked by the evolution hook.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionMetrics {
    pub turn_id: u64,
    pub timestamp: String,
    pub tool_call_count: u32,
    pub token_usage: u32,
    pub user_correction_rate: f64, // 0.0 to 1.0
    pub tool_call_efficiency: f64, // e.g., successful calls / total calls
}

/// Configuration for the evolution policy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionPolicy {
    pub enabled: bool,
    pub metrics: Vec<String>,
    pub threshold: EvolutionThresholds,
    pub action: String, // e.g., "propose_config_update"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvolutionThresholds {
    pub user_correction_rate: f64,
}

impl Default for EvolutionPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            metrics: vec!["tool_call_efficiency".into(), "user_correction_rate".into()],
            threshold: EvolutionThresholds {
                user_correction_rate: 0.2,
            },
            action: "propose_config_update".into(),
        }
    }
}

/// Hook that monitors agent performance and logs evolution data.
pub struct EvolutionHook {
    log_path: PathBuf,
    policy: EvolutionPolicy,
    turn_counter: u64,
}

impl EvolutionHook {
    pub fn new(root: &Path, policy: EvolutionPolicy) -> Self {
        let log_dir = root.join(".pisci");
        std::fs::create_dir_all(&log_dir).ok();
        Self {
            log_path: log_dir.join("evolution_log.jsonl"),
            policy,
            turn_counter: 0,
        }
    }

    /// Record metrics for the current turn.
    pub fn record_turn(&mut self, metrics: EvolutionMetrics) -> Result<()> {
        if !self.policy.enabled {
            return Ok(());
        }

        self.turn_counter += 1;

        // Append to JSONL log
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log_path)?;

        let line = serde_json::to_string(&metrics)?;
        writeln!(file, "{}", line)?;

        // Check thresholds
        if metrics.user_correction_rate > self.policy.threshold.user_correction_rate {
            // In a real system, this would trigger a notification or a tool call
            // For now, we just log it.
            eprintln!(
                "Evolution Trigger: High correction rate detected ({})",
                metrics.user_correction_rate
            );
        }

        Ok(())
    }

    /// Generate a proposed config update based on recent history.
    pub fn propose_update(&self) -> Option<String> {
        if !self.policy.enabled {
            return None;
        }

        // Stub: In a real implementation, this would analyze the log
        // and suggest specific YAML changes (e.g., refining system_prompt).
        Some("# Proposed Config Update\n# Refine system_prompt to be more concise\n".to_string())
    }
}

/// Adapter that bridges the workbench's YAML `hooks` configuration to the
/// kernel's [`AgentHooks`] extension surface.
///
/// The kernel invokes `before_tool` / `after_tool` / `on_context_event` at
/// fixed points in the agent loop. `WorkbenchHooks` translates the configured
/// hook types (`AuditLog`, `FileJournal`, `PlanTracker`, ...) into concrete
/// behavior: structured stderr auditing, an append-only JSONL journal, and a
/// running tool-call counter that downstream metrics can consume.
pub struct WorkbenchHooks {
    audit: bool,
    journal_path: Option<PathBuf>,
    tool_calls: Mutex<u64>,
}

impl Default for WorkbenchHooks {
    fn default() -> Self {
        Self {
            audit: false,
            journal_path: None,
            tool_calls: Mutex::new(0),
        }
    }
}

impl WorkbenchHooks {
    pub fn new() -> Self {
        Self::default()
    }

    /// Enable structured stderr auditing of tool and context events.
    pub fn with_audit(mut self, enabled: bool) -> Self {
        self.audit = enabled;
        self
    }

    /// Write an append-only JSONL journal of tool invocations to `path`.
    pub fn with_journal(mut self, path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        self.journal_path = Some(path);
        self
    }

    /// Number of tool invocations observed so far.
    pub fn tool_call_count(&self) -> u64 {
        *self.tool_calls.lock().unwrap()
    }

    /// Returns true if any observable behavior is wired (so callers can decide
    /// whether attaching this hook to the harness is worthwhile).
    pub fn is_active(&self) -> bool {
        self.audit || self.journal_path.is_some()
    }

    fn append_journal(&self, record: &serde_json::Value) {
        if let Some(path) = &self.journal_path {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{}", record);
            }
        }
    }
}

#[async_trait]
impl AgentHooks for WorkbenchHooks {
    async fn before_tool(&self, ev: &ToolHookEvent<'_>) -> HookDecision {
        {
            let mut count = self.tool_calls.lock().unwrap();
            *count += 1;
        }
        if self.audit {
            eprintln!(
                "[workbench:audit] before_tool name={} id={} session={}",
                ev.tool_name, ev.tool_use_id, ev.session_id
            );
        }
        self.append_journal(&serde_json::json!({
            "event": "before_tool",
            "session_id": ev.session_id,
            "tool_use_id": ev.tool_use_id,
            "tool_name": ev.tool_name,
            "input": ev.input,
        }));
        HookDecision::Continue
    }

    async fn after_tool(&self, ev: &ToolHookEvent<'_>, _result: &ToolResult) {
        if self.audit {
            eprintln!("[workbench:audit] after_tool name={}", ev.tool_name);
        }
        self.append_journal(&serde_json::json!({
            "event": "after_tool",
            "session_id": ev.session_id,
            "tool_use_id": ev.tool_use_id,
            "tool_name": ev.tool_name,
        }));
    }

    async fn on_context_event(&self, ev: &ContextHookEvent<'_>) {
        if !self.audit {
            return;
        }
        match ev {
            ContextHookEvent::TurnStart { session_id } => {
                eprintln!("[workbench:audit] turn_start session={}", session_id)
            }
            ContextHookEvent::TurnEnd { session_id } => {
                eprintln!("[workbench:audit] turn_end session={}", session_id)
            }
            ContextHookEvent::BeforeCompact {
                session_id,
                message_count,
            } => eprintln!(
                "[workbench:audit] before_compact session={} messages={}",
                session_id, message_count
            ),
            ContextHookEvent::AfterCompact {
                session_id,
                message_count,
            } => eprintln!(
                "[workbench:audit] after_compact session={} messages={}",
                session_id, message_count
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use tempfile::TempDir;

    #[test]
    fn test_workbench_hooks_journal_and_count() {
        let dir = TempDir::new().unwrap();
        let journal = dir.path().join(".pisci").join("journal.jsonl");
        let hooks = WorkbenchHooks::new()
            .with_audit(true)
            .with_journal(journal.clone());

        assert!(hooks.is_active());

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let input = serde_json::json!({ "path": "a.txt" });
            let ev = ToolHookEvent {
                session_id: "s1",
                tool_use_id: "t1",
                tool_name: "file_write",
                input: &input,
                workspace_root: dir.path(),
            };
            let decision = hooks.before_tool(&ev).await;
            assert!(matches!(decision, HookDecision::Continue));
        });

        assert_eq!(hooks.tool_call_count(), 1);
        assert!(journal.exists());
        let content = std::fs::read_to_string(&journal).unwrap();
        assert!(content.contains("\"event\":\"before_tool\""));
        assert!(content.contains("file_write"));
    }

    #[test]
    fn test_workbench_hooks_default_inactive() {
        let hooks = WorkbenchHooks::new();
        assert!(!hooks.is_active());
    }

    #[test]
    fn test_evolution_hook_logging() {
        let dir = TempDir::new().unwrap();
        let policy = EvolutionPolicy {
            enabled: true,
            ..Default::default()
        };
        let mut hook = EvolutionHook::new(dir.path(), policy);

        let metrics = EvolutionMetrics {
            turn_id: 1,
            timestamp: Utc::now().to_rfc3339(),
            tool_call_count: 5,
            token_usage: 1000,
            user_correction_rate: 0.1,
            tool_call_efficiency: 0.9,
        };

        hook.record_turn(metrics).unwrap();
        assert!(hook.log_path.exists());

        let content = std::fs::read_to_string(&hook.log_path).unwrap();
        assert!(content.contains("\"turn_id\":1"));
    }

    #[test]
    fn test_disabled_hook() {
        let dir = TempDir::new().unwrap();
        let policy = EvolutionPolicy {
            enabled: false,
            ..Default::default()
        };
        let mut hook = EvolutionHook::new(dir.path(), policy);

        let metrics = EvolutionMetrics {
            turn_id: 1,
            timestamp: Utc::now().to_rfc3339(),
            tool_call_count: 5,
            token_usage: 1000,
            user_correction_rate: 0.5,
            tool_call_efficiency: 0.5,
        };

        hook.record_turn(metrics).unwrap();
        // Should not create log file if disabled
        assert!(!hook.log_path.exists());
    }
}
