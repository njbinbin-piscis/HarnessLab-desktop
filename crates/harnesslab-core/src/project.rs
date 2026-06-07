//! Project / Agent / Topology data model.
//!
//! Mirrors HarnessLab's Python `project.model` (and the TypeScript `api.ts`
//! contract) so the ported React frontend round-trips unchanged over Tauri IPC.
//! A *project* holds one or more [`AgentSpec`]s, a [`Topology`] describing how
//! they are coordinated, dataset references and an optional scorer node.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// A reference to a library component plus its constructor params.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentRef {
    pub name: String,
    #[serde(default)]
    pub params: BTreeMap<String, serde_json::Value>,
}

impl ComponentRef {
    pub fn new(name: &str) -> Self {
        ComponentRef {
            name: name.to_string(),
            params: BTreeMap::new(),
        }
    }
}

fn default_loop() -> Option<ComponentRef> {
    Some(ComponentRef::new("react"))
}
fn default_compaction() -> Option<ComponentRef> {
    Some(ComponentRef::new("sliding_window"))
}
fn default_memory() -> Option<ComponentRef> {
    Some(ComponentRef::new("noop"))
}
fn default_context() -> Option<ComponentRef> {
    Some(ComponentRef::new("none"))
}
fn default_model() -> String {
    std::env::var("PISCIS_DEFAULT_MODEL").unwrap_or_else(|_| "qwen-plus".to_string())
}
fn default_credentials() -> Option<ComponentRef> {
    Some(ComponentRef::new("env"))
}
fn default_system_prompt() -> String {
    "You are a helpful agent.".to_string()
}
fn default_max_turns() -> u32 {
    6
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_context_window() -> u32 {
    200_000
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentSpec {
    pub name: String,
    #[serde(default)]
    pub role: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default = "default_max_turns")]
    pub max_turns: u32,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,
    /// 0 = unlimited; engine-enforced cost ceiling.
    #[serde(default)]
    pub max_total_tokens: u32,
    #[serde(default = "default_context_window")]
    pub context_window: u32,
    /// `None` = all built-in tools.
    #[serde(default)]
    pub enabled_tools: Option<Vec<String>>,

    #[serde(default = "default_loop")]
    pub r#loop: Option<ComponentRef>,
    #[serde(default = "default_compaction")]
    pub compaction: Option<ComponentRef>,
    #[serde(default = "default_memory")]
    pub memory: Option<ComponentRef>,
    #[serde(default = "default_context")]
    pub context: Option<ComponentRef>,

    // Harness lifecycle stages — empty slots by default (None = engine built-in
    // / transparent behaviour). Fill them to make a "thin" harness "thick".
    #[serde(default)]
    pub credentials: Option<ComponentRef>,
    #[serde(default)]
    pub capabilities: Option<ComponentRef>,
    #[serde(default)]
    pub skills: Option<ComponentRef>,
    #[serde(default)]
    pub prompt: Option<ComponentRef>,
    #[serde(default)]
    pub approval: Option<ComponentRef>,
    #[serde(default)]
    pub budget: Option<ComponentRef>,
    #[serde(default)]
    pub hooks: Option<ComponentRef>,
    #[serde(default)]
    pub session: Option<ComponentRef>,
    #[serde(default)]
    pub tracing: Option<ComponentRef>,
}

impl AgentSpec {
    /// A new agent with the default algorithm slots filled.
    pub fn new(name: &str) -> Self {
        AgentSpec {
            name: name.to_string(),
            role: String::new(),
            model: default_model(),
            system_prompt: default_system_prompt(),
            max_turns: default_max_turns(),
            max_tokens: default_max_tokens(),
            max_total_tokens: 0,
            context_window: default_context_window(),
            enabled_tools: None,
            r#loop: default_loop(),
            compaction: default_compaction(),
            memory: default_memory(),
            context: default_context(),
            credentials: default_credentials(),
            capabilities: None,
            skills: None,
            prompt: None,
            approval: None,
            budget: None,
            hooks: None,
            session: None,
            tracing: None,
        }
    }
}

/// How the project's agents combine into one answer. `mode` is `single` (one
/// agent) or `coordinated` (use a coordinator algorithm over `agents`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Topology {
    #[serde(default = "default_mode")]
    pub mode: String,
    #[serde(default)]
    pub coordinator: Option<ComponentRef>,
    #[serde(default)]
    pub agent_order: Vec<String>,
}

fn default_mode() -> String {
    "single".to_string()
}

impl Default for Topology {
    fn default() -> Self {
        Topology {
            mode: default_mode(),
            coordinator: None,
            agent_order: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub agents: Vec<AgentSpec>,
    #[serde(default)]
    pub topology: Topology,
    #[serde(default)]
    pub datasets: Vec<String>,
    #[serde(default)]
    pub scorer: Option<ComponentRef>,
    /// Monotonically bumped on each save; used for optimistic concurrency.
    #[serde(default)]
    pub rev: u64,
    #[serde(default)]
    pub created_at: f64,
    #[serde(default)]
    pub updated_at: f64,
}

impl Project {
    pub fn agent(&self, name: &str) -> Option<&AgentSpec> {
        self.agents.iter().find(|a| a.name == name)
    }

    /// Agents in topology order (falls back to declaration order).
    pub fn ordered_agents(&self) -> Vec<&AgentSpec> {
        if self.topology.agent_order.is_empty() {
            return self.agents.iter().collect();
        }
        self.topology
            .agent_order
            .iter()
            .filter_map(|n| self.agent(n))
            .collect()
    }
}

/// Current unix time in seconds (fractional), matching Python's `time.time()`.
pub fn now_secs() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a short, unique, hex-ish id (12 chars), mirroring the Python
/// `uuid4().hex[:12]` convention closely enough for local use.
pub fn new_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let counter = ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mixed = (nanos as u64) ^ (counter.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    format!("{mixed:012x}").chars().take(12).collect()
}
