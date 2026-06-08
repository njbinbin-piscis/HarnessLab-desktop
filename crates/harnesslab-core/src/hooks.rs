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
use harness_rt::agent::hooks::{AgentHooks, ContextHookEvent, HookDecision, ToolHookEvent};
use harness_rt::agent::tool::ToolResult;

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

impl From<&crate::config::EvolutionPolicyConfig> for EvolutionPolicy {
    fn from(cfg: &crate::config::EvolutionPolicyConfig) -> Self {
        Self {
            enabled: cfg.enabled,
            metrics: cfg.metrics.clone(),
            threshold: EvolutionThresholds {
                user_correction_rate: cfg.threshold.user_correction_rate,
            },
            action: cfg.action.clone(),
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

    /// Generate a proposed config update by analyzing the recorded metrics log.
    ///
    /// Reads the JSONL evolution log, aggregates the most recent turns, and emits
    /// concrete, data-driven YAML suggestions. Returns `None` when the policy is
    /// disabled or there is not enough history to make a recommendation.
    pub fn propose_update(&self) -> Option<String> {
        if !self.policy.enabled {
            return None;
        }

        let raw = std::fs::read_to_string(&self.log_path).ok()?;
        let recent: Vec<EvolutionMetrics> = raw
            .lines()
            .rev()
            .take(20)
            .filter_map(|l| serde_json::from_str::<EvolutionMetrics>(l).ok())
            .collect();
        if recent.is_empty() {
            return None;
        }

        let n = recent.len() as f64;
        let avg_correction = recent.iter().map(|m| m.user_correction_rate).sum::<f64>() / n;
        let avg_efficiency = recent.iter().map(|m| m.tool_call_efficiency).sum::<f64>() / n;
        let avg_tool_calls = recent.iter().map(|m| m.tool_call_count as f64).sum::<f64>() / n;
        let avg_tokens = recent.iter().map(|m| m.token_usage as f64).sum::<f64>() / n;

        let mut suggestions: Vec<String> = Vec::new();
        if avg_correction > self.policy.threshold.user_correction_rate {
            suggestions.push(format!(
                "# High correction rate ({avg_correction:.2}) — tighten the system_prompt with clearer constraints and an explicit self-check step."
            ));
        }
        if avg_efficiency < 0.7 {
            suggestions.push(format!(
                "# Low tool-call efficiency ({avg_efficiency:.2}) — narrow the available tools or add usage examples to the prompt."
            ));
        }
        if avg_tool_calls > 12.0 {
            suggestions.push(format!(
                "# High tool-call volume ({avg_tool_calls:.1}/turn) — consider a budgeted loop or a lower max_iterations."
            ));
        }
        if avg_tokens > 8000.0 {
            suggestions.push(format!(
                "# High token usage ({avg_tokens:.0}/turn) — enable summary compaction to control context growth."
            ));
        }

        if suggestions.is_empty() {
            return None;
        }
        Some(format!(
            "# Proposed Config Update (based on {} recent turns)\n{}\n",
            recent.len(),
            suggestions.join("\n")
        ))
    }
}

/// Per-turn counters collected for [`EvolutionHook`] metrics.
#[derive(Debug, Default, Clone)]
struct TurnStats {
    tools_total: u32,
    tools_ok: u32,
    token_est: u32,
}

/// Adapter that bridges the workbench's YAML `hooks` configuration to the
/// kernel's [`AgentHooks`] extension surface.
///
/// The kernel invokes `before_tool` / `after_tool` / `on_context_event` at
/// fixed points in the agent loop. `WorkbenchHooks` translates the configured
/// hook types (`AuditLog`, `FileJournal`, `PlanTracker`, ...) into concrete
/// behavior: structured stderr auditing, an append-only JSONL journal, a
/// running tool-call counter, and optional self-evolution metrics logging
/// driven by `policy.evolution`.
pub struct WorkbenchHooks {
    audit: bool,
    journal_path: Option<PathBuf>,
    plan_path: Option<PathBuf>,
    tool_calls: Mutex<u64>,
    evolution: Option<Mutex<EvolutionHook>>,
    turn: Mutex<TurnStats>,
}

impl Default for WorkbenchHooks {
    fn default() -> Self {
        Self {
            audit: false,
            journal_path: None,
            plan_path: None,
            tool_calls: Mutex::new(0),
            evolution: None,
            turn: Mutex::new(TurnStats::default()),
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

    /// Maintain a human-readable Markdown plan/progress log at `path`, with one
    /// checklist entry per tool invocation (queued on `before_tool`, marked done
    /// on `after_tool`).
    pub fn with_plan_tracker(mut self, path: PathBuf) -> Self {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).ok();
        }
        // Start a fresh plan section for this run.
        if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) {
            let _ = writeln!(file, "\n## Plan {}\n", crate::project::now_secs() as u64);
        }
        self.plan_path = Some(path);
        self
    }

    /// Enable self-evolution metrics logging under `<root>/.pisci/evolution_log.jsonl`.
    pub fn with_evolution(mut self, policy: EvolutionPolicy, root: PathBuf) -> Self {
        if policy.enabled {
            self.evolution = Some(Mutex::new(EvolutionHook::new(&root, policy)));
        }
        self
    }

    /// Read the latest evolution proposal, if any, from the configured log.
    pub fn propose_evolution_update(&self) -> Option<String> {
        self.evolution
            .as_ref()
            .and_then(|e| e.lock().ok())
            .and_then(|hook| hook.propose_update())
    }

    /// Number of tool invocations observed so far.
    pub fn tool_call_count(&self) -> u64 {
        *self.tool_calls.lock().unwrap()
    }

    /// Returns true if any observable behavior is wired (so callers can decide
    /// whether attaching this hook to the harness is worthwhile).
    pub fn is_active(&self) -> bool {
        self.audit
            || self.journal_path.is_some()
            || self.plan_path.is_some()
            || self.evolution.is_some()
    }

    fn append_journal(&self, record: &serde_json::Value) {
        if let Some(path) = &self.journal_path {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{}", record);
            }
        }
    }

    fn append_plan(&self, line: &str) {
        if let Some(path) = &self.plan_path {
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{}", line);
            }
        }
    }

    fn record_turn_metrics(&self) {
        let Some(evo) = &self.evolution else {
            return;
        };
        let stats = self.turn.lock().unwrap().clone();
        let total = stats.tools_total;
        let efficiency = if total == 0 {
            1.0
        } else {
            stats.tools_ok as f64 / total as f64
        };
        // Without explicit user feedback, treat tool failures as correction pressure.
        let correction = if total == 0 {
            0.0
        } else {
            (total - stats.tools_ok) as f64 / total as f64
        };
        let metrics = EvolutionMetrics {
            turn_id: 0,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tool_call_count: total,
            token_usage: stats.token_est,
            user_correction_rate: correction,
            tool_call_efficiency: efficiency,
        };
        if let Ok(mut hook) = evo.lock() {
            let _ = hook.record_turn(metrics);
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
        if self.evolution.is_some() {
            let mut turn = self.turn.lock().unwrap();
            turn.tools_total += 1;
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
        if self.plan_path.is_some() {
            let step = *self.tool_calls.lock().unwrap();
            let summary: String = ev.input.to_string().chars().take(80).collect();
            self.append_plan(&format!(
                "- [ ] step {step}: `{}` — {}",
                ev.tool_name, summary
            ));
        }
        HookDecision::Continue
    }

    async fn after_tool(&self, ev: &ToolHookEvent<'_>, result: &ToolResult) {
        if self.evolution.is_some() {
            let mut turn = self.turn.lock().unwrap();
            if !result.is_error {
                turn.tools_ok += 1;
            }
            turn.token_est += (result.content.len() as u32).saturating_add(1) / 4;
        }
        if self.audit {
            eprintln!("[workbench:audit] after_tool name={}", ev.tool_name);
        }
        self.append_journal(&serde_json::json!({
            "event": "after_tool",
            "session_id": ev.session_id,
            "tool_use_id": ev.tool_use_id,
            "tool_name": ev.tool_name,
        }));
        if self.plan_path.is_some() {
            let status = if result.is_error { "failed" } else { "done" };
            self.append_plan(&format!("  - ✓ {} ({status})", ev.tool_name));
        }
    }

    async fn on_context_event(&self, ev: &ContextHookEvent<'_>) {
        match ev {
            ContextHookEvent::TurnStart { session_id } => {
                *self.turn.lock().unwrap() = TurnStats::default();
                if self.audit {
                    eprintln!("[workbench:audit] turn_start session={session_id}");
                }
            }
            ContextHookEvent::TurnEnd { session_id } => {
                self.record_turn_metrics();
                if self.audit {
                    eprintln!("[workbench:audit] turn_end session={session_id}");
                }
            }
            ContextHookEvent::BeforeCompact {
                session_id,
                message_count,
            } if self.audit => eprintln!(
                "[workbench:audit] before_compact session={session_id} messages={message_count}"
            ),
            ContextHookEvent::AfterCompact {
                session_id,
                message_count,
            } if self.audit => eprintln!(
                "[workbench:audit] after_compact session={session_id} messages={message_count}"
            ),
            _ => {}
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
    fn test_workbench_hooks_evolution_only_active() {
        let dir = TempDir::new().unwrap();
        let policy = EvolutionPolicy {
            enabled: true,
            ..Default::default()
        };
        let hooks = WorkbenchHooks::new().with_evolution(policy, dir.path().to_path_buf());
        assert!(hooks.is_active());
    }

    #[test]
    fn test_workbench_hooks_evolution_records_turn() {
        let dir = TempDir::new().unwrap();
        let policy = EvolutionPolicy {
            enabled: true,
            ..Default::default()
        };
        let hooks = WorkbenchHooks::new().with_evolution(policy, dir.path().to_path_buf());
        let log_path = dir.path().join(".pisci").join("evolution_log.jsonl");

        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            hooks
                .on_context_event(&ContextHookEvent::TurnStart { session_id: "s1" })
                .await;

            let input = serde_json::json!({ "path": "a.txt" });
            let ev = ToolHookEvent {
                session_id: "s1",
                tool_use_id: "t1",
                tool_name: "file_write",
                input: &input,
                workspace_root: dir.path(),
            };
            hooks.before_tool(&ev).await;
            hooks.after_tool(&ev, &ToolResult::ok("wrote file")).await;

            hooks
                .on_context_event(&ContextHookEvent::TurnEnd { session_id: "s1" })
                .await;
        });

        assert!(log_path.exists());
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("\"tool_call_count\":1"));
        assert!(content.contains("\"tool_call_efficiency\":1"));
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
