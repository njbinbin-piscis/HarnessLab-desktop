//! Live project execution over `piscis-kernel` (P3).
//!
//! Bridges the visual [`Project`] model to the kernel agent loop: it maps each
//! agent's slots to a [`AgentConfig`], builds the harness, runs the task and
//! **translates** every kernel [`AgentEvent`] into the flat
//! `{kind, agent, text, data, ts}` shape the ported frontend console + metrics
//! already understand.
//!
//! Single-agent topology runs one kernel loop; coordinated topology delegates to
//! [`crate::coordinator`] which orchestrates multiple agent runs.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use harness_rt::agent::loop_::AgentLoop;
use harness_rt::agent::messages::AgentEvent;
use harness_rt::agent::tool::{ToolContext, ToolRegistry, ToolSettings};
use harness_rt::llm::{build_client_with_timeout, LlmMessage, MessageContent};
use harness_rt::policy::PolicyGate;
use harness_rt::tools::{register_neutral_into, NeutralToolsConfig};
use serde::Serialize;
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::config::{
    AgentConfig, AgentDefinition, CompressionConfig, HookConfig, PluginConfig, PolicyConfig,
    StrategyConfig,
};
use crate::loader::build_harness_config;
use crate::project::{now_secs, AgentSpec, Project};

mod coordinator;

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

/// Shared context passed to each agent invocation during a coordinated run.
#[derive(Clone)]
pub struct AgentRunContext {
    pub workspace: PathBuf,
    pub events: mpsc::Sender<UiEvent>,
    pub cancel: Arc<AtomicBool>,
}

/// Outcome of one agent kernel loop (used by coordinators to aggregate tokens).
#[derive(Debug, Clone, Default)]
pub struct AgentRunOutcome {
    pub result: String,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

// ── Project → AgentConfig mapping ────────────────────────────────────────────

fn slot_name<'a>(slot: &'a Option<crate::project::ComponentRef>) -> Option<&'a str> {
    slot.as_ref().map(|c| c.name.as_str())
}

/// Map the project's loop component to a runtime loop-strategy id.
///
/// All catalog loop names resolve to a real strategy; an empty slot defaults to
/// `react`. Unknown names pass through so the harness builder rejects them
/// loudly rather than silently degrading.
fn map_loop(spec: &AgentSpec) -> String {
    slot_name(&spec.r#loop).unwrap_or("react").to_string()
}

/// Map the project's compaction component to a runtime compaction-strategy id.
fn map_compaction(spec: &AgentSpec) -> String {
    match slot_name(&spec.compaction) {
        None => "summary_based",
        Some("summary") => "summary_based",
        Some("none") => "none",
        Some("sliding_window") => "sliding_window",
        Some("vector_retrieval") => "vector_retrieval",
        Some(other) => other,
    }
    .to_string()
}

/// Map the project's memory component to a runtime memory-plugin id.
fn map_memory(spec: &AgentSpec) -> String {
    match slot_name(&spec.memory) {
        None | Some("noop") => "NoOpMemory",
        Some("in_memory_vector") => "InMemoryVectorStore",
        // Both persistent slots resolve to the SQLite-backed store.
        Some("sqlite") | Some("persistent_vector") => "SqliteMemory",
        Some(other) => other,
    }
    .to_string()
}

/// Map the project's context component to a runtime context-manager id.
fn map_context(spec: &AgentSpec) -> String {
    match slot_name(&spec.context) {
        None | Some("project_context") => "ProjectContextManager",
        Some("workspace_root") => "WorkspaceRootManager",
        Some("rag") => "DynamicContextManager",
        Some("none") => "NoOpContextManager",
        Some(other) => other,
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
        format!(
            "You are {}. Follow the user's instructions carefully.",
            spec.name
        )
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

/// Build the tool registry for an agent, honoring its `enabled_tools` whitelist.
///
/// `None` (absent) registers all built-in tools; `Some(list)` registers exactly
/// the named tools (an empty list yields a reasoning-only, tool-free agent).
/// The runtime additionally filters out tools whose platform does not match the
/// host OS.
fn agent_tool_registry(enabled_tools: Option<&[String]>) -> ToolRegistry {
    let mut registry = ToolRegistry::new();
    let mut cfg = NeutralToolsConfig::default();
    if let Some(list) = enabled_tools {
        let map: std::collections::HashMap<String, bool> =
            list.iter().map(|t| (t.clone(), true)).collect();
        cfg.builtin_tool_enabled = Some(map);
    }
    register_neutral_into(&mut registry, &cfg);
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
        AgentEvent::Error { message } => UiEvent::new(
            agent,
            "error",
            message.clone(),
            json!({ "message": message }),
        ),
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

/// Run one agent's kernel loop, streaming translated events tagged with its name.
pub async fn run_agent_task(
    ctx: &AgentRunContext,
    spec: &AgentSpec,
    task: &str,
    context: &str,
) -> Result<AgentRunOutcome, String> {
    crate::contrib::register_all();

    let resolved = crate::stages::credentials::resolve_for_agent(spec)?;
    let mut run_spec = spec.clone();
    run_spec.model = resolved.model.clone();

    let agent_name = spec.name.clone();
    let cfg = agent_config_for(&run_spec);
    let registry = Arc::new(agent_tool_registry(cfg.enabled_tools.as_deref()));
    let policy = Arc::new(PolicyGate::new(ctx.workspace.clone()));
    let harness = build_harness_config(&cfg, registry, policy, Some(&ctx.workspace))
        .map_err(|e| format!("构建 harness 失败 ({agent_name}): {e}"))?;

    let model = harness.model.clone();
    let client = build_client_with_timeout(
        &resolved.creds.provider,
        &resolved.creds.api_key,
        resolved.creds.base_url.as_deref(),
        120,
    );
    let agent: Arc<AgentLoop> = Arc::new(harness.into_agent_loop(client, None, None));

    let _ = ctx
        .events
        .send(UiEvent::new(
            &agent_name,
            "run_start",
            "",
            json!({ "model": model, "provider": resolved.creds.provider }),
        ))
        .await;

    let user_content = if context.is_empty() {
        task.to_string()
    } else {
        format!("{task}\n\n## Shared context\n{context}")
    };

    let (tx, mut rx) = mpsc::channel::<AgentEvent>(256);
    let tool_ctx = make_tool_context(&ctx.workspace, &agent_name, ctx.cancel.clone());
    let messages = vec![LlmMessage {
        role: "user".to_string(),
        content: MessageContent::text(&user_content),
    }];

    let agent_run = agent.clone();
    let cancel_run = ctx.cancel.clone();
    let handle =
        tokio::spawn(async move { agent_run.run(messages, tx, cancel_run, tool_ctx).await });

    let mut segment = String::new();
    let mut last_answer = String::new();
    let mut total_in = 0u32;
    let mut total_out = 0u32;
    while let Some(ev) = rx.recv().await {
        if let AgentEvent::Done {
            total_input_tokens,
            total_output_tokens,
        } = &ev
        {
            total_in = *total_input_tokens;
            total_out = *total_output_tokens;
        }
        if let Some(ui) = translate(&agent_name, &ev, &mut segment) {
            if !segment.trim().is_empty() {
                last_answer = segment.clone();
            }
            let _ = ctx.events.send(ui).await;
        }
    }

    match handle.await {
        Ok(Ok((_msgs, in_t, out_t))) => Ok(AgentRunOutcome {
            result: last_answer,
            total_input_tokens: in_t.max(total_in),
            total_output_tokens: out_t.max(total_out),
        }),
        Ok(Err(e)) => Err(format!("agent {agent_name} failed: {e}")),
        Err(e) => Err(format!("agent {agent_name} panicked: {e}")),
    }
}

/// Run a task against the project — single agent or coordinated multi-agent —
/// streaming translated events to `events`.
pub async fn run_project_task(
    project: &Project,
    task: &str,
    workspace: &Path,
    events: mpsc::Sender<UiEvent>,
    cancel: Arc<AtomicBool>,
) -> Result<RunOutcome, String> {
    let specs = project.ordered_agents();
    if specs.is_empty() {
        return Err("项目没有可运行的 agent".to_string());
    }

    // Fail fast if the primary agent cannot resolve credentials.
    crate::stages::credentials::resolve_for_agent(specs[0])?;

    let ctx = AgentRunContext {
        workspace: workspace.to_path_buf(),
        events: events.clone(),
        cancel,
    };

    let topo = &project.topology;
    let outcome = if topo.mode == "single" || specs.len() == 1 || topo.coordinator.is_none() {
        let spec = &specs[0];
        run_agent_task(&ctx, spec, task, "").await?
    } else {
        let coord = topo.coordinator.as_ref().unwrap();
        let _ = events
            .send(UiEvent::new(
                "__coordinator__",
                "coord_start",
                &coord.name,
                json!({ "mode": topo.mode, "agents": specs.iter().map(|s| &s.name).collect::<Vec<_>>() }),
            ))
            .await;
        let owned: Vec<AgentSpec> = specs.iter().map(|s| (*s).clone()).collect();
        let result =
            coordinator::coordinate(&coord.name, &coord.params, &owned, task, &ctx).await?;
        let _ = events
            .send(UiEvent::new(
                "__coordinator__",
                "coord_end",
                "",
                json!({ "coordinator": coord.name }),
            ))
            .await;
        result
    };

    Ok(RunOutcome {
        result: outcome.result,
        total_input_tokens: outcome.total_input_tokens,
        total_output_tokens: outcome.total_output_tokens,
    })
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

pub fn library_agent_workspace(base: &Path, agent_id: &str) -> PathBuf {
    base.join("library-agents").join(agent_id)
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
    fn config_maps_real_slots_without_silent_fallback() {
        // Every catalog slot maps to a real runtime id (no silent rewrite).
        let mut spec = AgentSpec::new("a");
        spec.r#loop = Some(ComponentRef::new("rewoo"));
        spec.compaction = Some(ComponentRef::new("none"));
        spec.memory = Some(ComponentRef::new("persistent_vector"));
        spec.context = Some(ComponentRef::new("none"));
        let cfg = agent_config_for(&spec);
        assert_eq!(cfg.strategy.strategy_type, "rewoo");
        assert_eq!(cfg.compression.strategy, "none");
        assert_eq!(cfg.plugins.memory, "SqliteMemory");
        assert_eq!(cfg.plugins.context, "NoOpContextManager");

        // The default AgentSpec slots map to their real runtime ids.
        let bare = AgentSpec::new("b");
        let cfg = agent_config_for(&bare);
        assert_eq!(cfg.strategy.strategy_type, "react");
        assert_eq!(cfg.compression.strategy, "sliding_window");
        assert_eq!(cfg.plugins.memory, "NoOpMemory");
        assert_eq!(cfg.plugins.context, "NoOpContextManager");
    }

    #[test]
    fn unknown_slot_names_fail_loudly() {
        // An unknown loop name passes through and the harness builder rejects it.
        let mut spec = AgentSpec::new("a");
        spec.r#loop = Some(ComponentRef::new("telepathy"));
        let cfg = agent_config_for(&spec);
        let reg = Arc::new(ToolRegistry::new());
        let policy = Arc::new(PolicyGate::new(std::path::PathBuf::from(".")));
        assert!(build_harness_config(&cfg, reg, policy, Some(Path::new("."))).is_err());
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
