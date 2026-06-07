//! Live project execution over `piscis-kernel` (P3).
//!
//! Bridges the visual [`Project`] model to the kernel agent loop: it maps the
//! primary agent's slots to a [`AgentConfig`], builds the harness, runs the
//! task and **translates** every kernel [`AgentEvent`] into the flat
//! `{kind, agent, text, data, ts}` shape the ported frontend console + metrics
//! already understand.
//!
//! P3 runs the project's *primary* agent (single mode, or the first agent in
//! topology order). Multi-agent coordination is a later increment.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use piscis_kernel::agent::loop_::AgentLoop;
use piscis_kernel::agent::messages::AgentEvent;
use piscis_kernel::agent::tool::{ToolContext, ToolRegistry, ToolSettings};
use piscis_kernel::llm::{build_client_with_timeout, LlmMessage, MessageContent};
use piscis_kernel::policy::PolicyGate;
use piscis_kernel::tools::{register_neutral_into, NeutralToolsConfig};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::config::{
    AgentConfig, AgentDefinition, CompressionConfig, HookConfig, PluginConfig, PolicyConfig,
    StrategyConfig,
};
use crate::loader::build_harness_config;
use crate::project::{now_secs, AgentSpec, Project};

/// LLM credentials, resolved from the environment for now (a settings UI lands
/// later). Mirrors the headless CLI's env contract.
#[derive(Debug, Clone)]
pub struct LlmCreds {
    pub provider: String,
    pub api_key: String,
    pub base_url: Option<String>,
}

impl LlmCreds {
    pub fn from_env() -> Result<Self, String> {
        let provider = std::env::var("PISCIS_PROVIDER").unwrap_or_else(|_| "qwen".to_string());
        let api_key = std::env::var("PISCIS_API_KEY")
            .or_else(|_| std::env::var("PISCIS_LLM_API_KEY"))
            .unwrap_or_default();
        if api_key.trim().is_empty() {
            return Err(
                "未配置 LLM 凭据:请设置环境变量 PISCIS_API_KEY(可选 PISCIS_PROVIDER / PISCIS_BASE_URL)"
                    .to_string(),
            );
        }
        let base_url = std::env::var("PISCIS_BASE_URL")
            .ok()
            .filter(|s| !s.is_empty());
        Ok(Self {
            provider,
            api_key,
            base_url,
        })
    }
}

/// A frontend-facing event — the flat shape the React console + `computeMetrics`
/// already consume (`{kind, agent, text, data, ts}`).
#[derive(Debug, Clone, Serialize)]
pub struct UiEvent {
    pub kind: String,
    pub agent: String,
    pub text: String,
    pub data: Value,
    pub ts: f64,
}

impl UiEvent {
    fn new(agent: &str, kind: &str, text: impl Into<String>, data: Value) -> Self {
        UiEvent {
            kind: kind.to_string(),
            agent: agent.to_string(),
            text: text.into(),
            data,
            ts: now_secs(),
        }
    }
}

/// The terminal result of a run.
pub struct RunOutcome {
    pub result: String,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

// ── Project → AgentConfig mapping ────────────────────────────────────────────

fn slot_name<'a>(slot: &'a Option<crate::project::ComponentRef>) -> Option<&'a str> {
    slot.as_ref().map(|c| c.name.as_str())
}

/// Map the project's loop component to a kernel loop-strategy id, falling back
/// to `react` when the name is not one the kernel resolves (so the harness
/// builder never rejects an experimental palette entry).
fn map_loop(spec: &AgentSpec) -> String {
    let name = slot_name(&spec.r#loop).unwrap_or("react");
    use piscis_kernel::agent::loop_strategy::resolve_loop_strategy;
    if resolve_loop_strategy(name).is_some() {
        name.to_string()
    } else {
        "react".to_string()
    }
}

/// Map the project's compaction component to a kernel compaction-strategy id.
fn map_compaction(spec: &AgentSpec) -> String {
    let id = match slot_name(&spec.compaction) {
        Some("summary") => "summary_based",
        Some("sliding_window") => "sliding_window",
        Some("vector_retrieval") => "vector_retrieval",
        _ => "summary_based",
    };
    use piscis_kernel::agent::loop_::resolve_compaction_strategy;
    if resolve_compaction_strategy(id, None).is_some() {
        id.to_string()
    } else {
        "summary_based".to_string()
    }
}

/// Map the project's memory component to a kernel memory-plugin id.
fn map_memory(spec: &AgentSpec) -> String {
    match slot_name(&spec.memory) {
        Some("in_memory_vector") => "InMemoryVectorStore",
        Some("sqlite") => "SqliteMemory",
        // Persistent vector store has no kernel backend yet → session-scoped.
        Some("persistent_vector") => "InMemoryVectorStore",
        _ => "NoOpMemory",
    }
    .to_string()
}

/// Map the project's context component to a kernel context-manager id.
fn map_context(spec: &AgentSpec) -> String {
    match slot_name(&spec.context) {
        Some("workspace_root") => "WorkspaceRootManager",
        Some("rag") => "DynamicContextManager",
        // "project_context", "none" and anything else → project scan (harmless).
        _ => "ProjectContextManager",
    }
    .to_string()
}

/// Build the YAML-shaped [`AgentConfig`] for one project agent.
pub fn agent_config_for(spec: &AgentSpec) -> AgentConfig {
    let system_prompt = if !spec.system_prompt.trim().is_empty() {
        spec.system_prompt.clone()
    } else if !spec.role.trim().is_empty() {
        spec.role.clone()
    } else {
        format!("You are {}. Follow the user's instructions carefully.", spec.name)
    };

    AgentConfig {
        agent: AgentDefinition {
            name: spec.name.clone(),
            model: spec.model.clone(),
            context_window: spec.context_window,
            max_tokens: spec.max_tokens,
            fallback_models: Vec::new(),
            enable_streaming: true,
            vision_override: None,
            vision_model: None,
        },
        plugins: PluginConfig {
            memory: map_memory(spec),
            context: map_context(spec),
            memory_retrieval_prompt: None,
        },
        policy: PolicyConfig::default(),
        strategy: StrategyConfig {
            strategy_type: map_loop(spec),
            system_prompt_template: system_prompt,
        },
        compression: CompressionConfig {
            strategy: map_compaction(spec),
            summarizer_prompt: None,
        },
        enabled_tools: spec.enabled_tools.clone(),
        hooks: Vec::<HookConfig>::new(),
    }
}

// ── Run execution ────────────────────────────────────────────────────────────

fn default_tool_registry() -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    register_neutral_into(&mut registry, &NeutralToolsConfig::default());
    registry
}

fn make_tool_context(workspace: &Path, agent_name: &str, cancel: Arc<AtomicBool>) -> ToolContext {
    ToolContext {
        session_id: format!("harnesslab-{agent_name}-{}", std::process::id()),
        workspace_root: workspace.to_path_buf(),
        bypass_permissions: true,
        settings: Arc::new(ToolSettings::default()),
        max_iterations: Some(50),
        memory_owner_id: "harnesslab".to_string(),
        pool_session_id: None,
        tool_use_id: None,
        cancel,
    }
}

/// Truncate long tool output for the live console (full output stays in logs).
fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let head: String = s.chars().take(max).collect();
        format!("{head}… (+{} chars)", s.chars().count() - max)
    }
}

/// Translate a kernel [`AgentEvent`] into the flat frontend event shape.
///
/// `segment` accumulates the text of the *current* assistant segment so the
/// caller can recover the final answer (the last segment after the last tool).
/// Returns `None` for events the console does not surface.
fn translate(agent: &str, ev: &AgentEvent, segment: &mut String) -> Option<UiEvent> {
    Some(match ev {
        AgentEvent::TextSegmentStart { iteration } => {
            segment.clear();
            UiEvent::new(agent, "turn_start", "", json!({ "iteration": iteration }))
        }
        AgentEvent::TextDelta { delta } => {
            segment.push_str(delta);
            UiEvent::new(agent, "text", delta.clone(), json!({}))
        }
        AgentEvent::ToolStart { id, name, input } => UiEvent::new(
            agent,
            "tool_call",
            name.clone(),
            json!({ "id": id, "name": name, "input": input }),
        ),
        AgentEvent::ToolEnd {
            id,
            name,
            result,
            is_error,
        } => UiEvent::new(
            agent,
            "tool_result",
            truncate(result, 400),
            json!({ "id": id, "name": name, "is_error": is_error }),
        ),
        AgentEvent::Done {
            total_input_tokens,
            total_output_tokens,
        } => UiEvent::new(
            agent,
            "run_end",
            "",
            json!({
                "total_tokens": total_input_tokens + total_output_tokens,
                "total_input_tokens": total_input_tokens,
                "total_output_tokens": total_output_tokens,
            }),
        ),
        AgentEvent::Cancelled => UiEvent::new(agent, "cancelled", "run cancelled", json!({})),
        AgentEvent::Error { message } => {
            UiEvent::new(agent, "error", message.clone(), json!({ "message": message }))
        }
        AgentEvent::PlanUpdate { items } => UiEvent::new(
            agent,
            "plan",
            "",
            json!({ "items": serde_json::to_value(items).unwrap_or(Value::Null) }),
        ),
        AgentEvent::ContextUsage {
            estimated_input_tokens,
            cumulative_input_tokens,
            cumulative_output_tokens,
            ..
        } => UiEvent::new(
            agent,
            "context_usage",
            "",
            json!({
                "estimated_input_tokens": estimated_input_tokens,
                "cumulative_input_tokens": cumulative_input_tokens,
                "cumulative_output_tokens": cumulative_output_tokens,
            }),
        ),
        // Events the desktop console does not surface yet.
        AgentEvent::MessageCommit { .. }
        | AgentEvent::PermissionRequest { .. }
        | AgentEvent::InteractiveUi { .. }
        | AgentEvent::InteractiveUiPatch { .. }
        | AgentEvent::InteractiveUiListen { .. }
        | AgentEvent::FishProgress { .. } => return None,
    })
}

/// Run a task against the project's primary agent, streaming translated events
/// to `events` and returning the final outcome.
pub async fn run_project_task(
    project: &Project,
    task: &str,
    creds: &LlmCreds,
    workspace: &Path,
    events: mpsc::Sender<UiEvent>,
    cancel: Arc<AtomicBool>,
) -> Result<RunOutcome, String> {
    // Install contributed strategies so palette loop/compaction names resolve.
    crate::contrib::register_all();

    let spec = project
        .ordered_agents()
        .into_iter()
        .next()
        .ok_or_else(|| "项目没有可运行的 agent".to_string())?;
    let agent_name = spec.name.clone();

    let cfg = agent_config_for(spec);
    let registry = Arc::new(default_tool_registry());
    let policy = Arc::new(PolicyGate::new(workspace.to_path_buf()));
    let harness = build_harness_config(&cfg, registry, policy, Some(workspace))
        .map_err(|e| format!("构建 harness 失败: {e}"))?;

    let model = harness.model.clone();
    let client = build_client_with_timeout(
        &creds.provider,
        &creds.api_key,
        creds.base_url.as_deref(),
        120,
    );
    let agent: Arc<AgentLoop> = Arc::new(harness.into_agent_loop(client, None, None));

    let _ = events
        .send(UiEvent::new(
            &agent_name,
            "run_start",
            "",
            json!({ "model": model, "provider": creds.provider }),
        ))
        .await;

    let (tx, mut rx) = mpsc::channel::<AgentEvent>(256);
    let ctx = make_tool_context(workspace, &agent_name, cancel.clone());
    let messages = vec![LlmMessage {
        role: "user".to_string(),
        content: MessageContent::text(task),
    }];

    let agent_run = agent.clone();
    let cancel_run = cancel.clone();
    let handle =
        tokio::spawn(async move { agent_run.run(messages, tx, cancel_run, ctx).await });

    let mut segment = String::new();
    let mut last_answer = String::new();
    while let Some(ev) = rx.recv().await {
        if let Some(ui) = translate(&agent_name, &ev, &mut segment) {
            // Snapshot the latest assistant segment as the running answer.
            if !segment.trim().is_empty() {
                last_answer = segment.clone();
            }
            let _ = events.send(ui).await;
        }
    }

    match handle.await {
        Ok(Ok((_msgs, total_in, total_out))) => Ok(RunOutcome {
            result: last_answer,
            total_input_tokens: total_in,
            total_output_tokens: total_out,
        }),
        Ok(Err(e)) => Err(format!("agent loop failed: {e}")),
        Err(e) => Err(format!("agent task panicked: {e}")),
    }
}

/// Per-agent run metrics, mirroring the frontend `computeMetrics` so a saved
/// run renders the same overlay as the live view.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize)]
pub struct NodeMetrics {
    pub turns: u32,
    pub tools: u32,
    pub tokens: u32,
    pub ms: u64,
}

/// Aggregate per-agent metrics from a run's translated events.
pub fn metrics_from_events(events: &[UiEvent]) -> HashMap<String, NodeMetrics> {
    let mut metrics: HashMap<String, NodeMetrics> = HashMap::new();
    let mut span: HashMap<String, (f64, f64)> = HashMap::new();
    for e in events {
        let m = metrics.entry(e.agent.clone()).or_default();
        match e.kind.as_str() {
            "turn_start" => m.turns += 1,
            "tool_call" => m.tools += 1,
            "run_end" => {
                m.tokens += e
                    .data
                    .get("total_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
            }
            _ => {}
        }
        let s = span.entry(e.agent.clone()).or_insert((e.ts, e.ts));
        s.0 = s.0.min(e.ts);
        s.1 = s.1.max(e.ts);
    }
    for (agent, m) in metrics.iter_mut() {
        if let Some((lo, hi)) = span.get(agent) {
            m.ms = ((hi - lo) * 1000.0).round() as u64;
        }
    }
    metrics
}

// Re-exported so the desktop crate can resolve a per-project workspace dir.
pub fn project_workspace(base: &Path, project_id: &str) -> PathBuf {
    base.join("workspaces").join(project_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ComponentRef;

    #[test]
    fn config_maps_known_slots() {
        let mut spec = AgentSpec::new("a");
        spec.model = "gpt-4o".into();
        spec.r#loop = Some(ComponentRef::new("plan_and_solve"));
        spec.compaction = Some(ComponentRef::new("summary"));
        spec.memory = Some(ComponentRef::new("in_memory_vector"));
        spec.context = Some(ComponentRef::new("workspace_root"));
        let cfg = agent_config_for(&spec);
        assert_eq!(cfg.agent.model, "gpt-4o");
        assert_eq!(cfg.strategy.strategy_type, "plan_and_solve");
        assert_eq!(cfg.compression.strategy, "summary_based");
        assert_eq!(cfg.plugins.memory, "InMemoryVectorStore");
        assert_eq!(cfg.plugins.context, "WorkspaceRootManager");
    }

    #[test]
    fn config_falls_back_for_unknown_or_none_slots() {
        let mut spec = AgentSpec::new("a");
        spec.r#loop = Some(ComponentRef::new("nonsense_loop"));
        spec.compaction = Some(ComponentRef::new("none"));
        spec.memory = Some(ComponentRef::new("noop"));
        spec.context = Some(ComponentRef::new("none"));
        let cfg = agent_config_for(&spec);
        assert_eq!(cfg.strategy.strategy_type, "react");
        assert_eq!(cfg.compression.strategy, "summary_based");
        assert_eq!(cfg.plugins.memory, "NoOpMemory");
        assert_eq!(cfg.plugins.context, "ProjectContextManager");
    }

    #[test]
    fn config_built_harness_is_constructible() {
        // The mapped config must always build a harness (never reject a slot).
        let spec = AgentSpec::new("solver");
        let cfg = agent_config_for(&spec);
        let reg = Arc::new(ToolRegistry::new());
        let policy = Arc::new(PolicyGate::new(std::path::PathBuf::from(".")));
        assert!(build_harness_config(&cfg, reg, policy, Some(Path::new("."))).is_ok());
    }

    #[test]
    fn metrics_aggregate_turns_tools_tokens() {
        let ev = |kind: &str, data: Value| UiEvent {
            kind: kind.to_string(),
            agent: "solver".to_string(),
            text: String::new(),
            data,
            ts: 1.0,
        };
        let events = vec![
            ev("turn_start", json!({})),
            ev("tool_call", json!({ "name": "shell" })),
            ev("tool_result", json!({})),
            ev("turn_start", json!({})),
            ev("run_end", json!({ "total_tokens": 1234 })),
        ];
        let m = metrics_from_events(&events);
        let s = m.get("solver").unwrap();
        assert_eq!(s.turns, 2);
        assert_eq!(s.tools, 1);
        assert_eq!(s.tokens, 1234);
    }
}
