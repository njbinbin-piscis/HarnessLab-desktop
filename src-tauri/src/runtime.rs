//! In-process live-run registry. Replaces the web build's HTTP/SSE runtime
//! monitor: each active run is tracked here and broadcast to the frontend via a
//! Tauri `runtime_update` event, plus exposed on demand through `runtime_runs`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use harnesslab_core::project::now_secs;
use harnesslab_core::runner::UiEvent;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

const MAX_RUNS: usize = 60;

#[derive(Clone, Default, Serialize)]
pub struct AgentRunState {
    pub agent: String,
    pub phase: String,
    pub turn: u32,
    pub tools: u32,
    pub tokens: u32,
    pub last_text: String,
}

#[derive(Clone, Serialize)]
pub struct RuntimeRun {
    pub run_id: String,
    pub project_id: String,
    pub task: String,
    pub status: String,
    pub started_at: f64,
    pub updated_at: f64,
    pub agents: HashMap<String, AgentRunState>,
    pub result: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Serialize)]
pub struct RuntimeSnapshot {
    pub version: u64,
    pub active: usize,
    pub runs: Vec<RuntimeRun>,
}

struct Registry {
    runs: HashMap<String, RuntimeRun>,
    order: Vec<String>,
    version: u64,
}

fn registry() -> &'static Mutex<Registry> {
    static REG: OnceLock<Mutex<Registry>> = OnceLock::new();
    REG.get_or_init(|| {
        Mutex::new(Registry {
            runs: HashMap::new(),
            order: Vec::new(),
            version: 0,
        })
    })
}

fn snapshot_locked(reg: &Registry) -> RuntimeSnapshot {
    let runs: Vec<RuntimeRun> = reg
        .order
        .iter()
        .filter_map(|id| reg.runs.get(id).cloned())
        .collect();
    let active = runs.iter().filter(|r| r.status == "running").count();
    RuntimeSnapshot {
        version: reg.version,
        active,
        runs,
    }
}

/// Current snapshot for the `runtime_runs` command.
pub fn snapshot() -> RuntimeSnapshot {
    snapshot_locked(&registry().lock().unwrap())
}

fn broadcast(app: &AppHandle) {
    let snap = snapshot();
    let _ = app.emit("runtime_update", &snap);
}

/// Register a new live run.
pub fn start(app: &AppHandle, run_id: &str, project_id: &str, task: &str) {
    {
        let mut reg = registry().lock().unwrap();
        reg.version += 1;
        let now = now_secs();
        reg.runs.insert(
            run_id.to_string(),
            RuntimeRun {
                run_id: run_id.to_string(),
                project_id: project_id.to_string(),
                task: task.chars().take(200).collect(),
                status: "running".to_string(),
                started_at: now,
                updated_at: now,
                agents: HashMap::new(),
                result: None,
                error: None,
            },
        );
        reg.order.push(run_id.to_string());
        prune(&mut reg);
    }
    broadcast(app);
}

/// Fold a streamed agent event into the run's per-agent state.
pub fn ingest(app: &AppHandle, run_id: &str, ev: &UiEvent) {
    {
        let mut reg = registry().lock().unwrap();
        reg.version += 1;
        let Some(run) = reg.runs.get_mut(run_id) else {
            return;
        };
        run.updated_at = now_secs();
        let agent = if ev.agent.is_empty() {
            "agent".to_string()
        } else {
            ev.agent.clone()
        };
        let st = run
            .agents
            .entry(agent.clone())
            .or_insert_with(|| AgentRunState {
                agent,
                ..Default::default()
            });
        match ev.kind.as_str() {
            "turn_start" => {
                st.turn += 1;
                st.phase = "think".to_string();
            }
            "text" => {
                st.phase = "think".to_string();
                if !ev.text.is_empty() {
                    st.last_text = ev.text.chars().take(160).collect();
                }
            }
            "tool_call" => {
                st.tools += 1;
                st.phase = "tool".to_string();
            }
            "run_end" => {
                st.phase = "done".to_string();
                if let Some(tok) = ev.data.get("total_tokens").and_then(|v| v.as_u64()) {
                    st.tokens = tok as u32;
                }
            }
            "cancelled" | "error" => st.phase = "done".to_string(),
            _ => {}
        }
    }
    broadcast(app);
}

/// Mark a run finished (terminal state) with an optional result/error.
pub fn finish(app: &AppHandle, run_id: &str, result: Option<String>, error: Option<String>) {
    {
        let mut reg = registry().lock().unwrap();
        reg.version += 1;
        if let Some(run) = reg.runs.get_mut(run_id) {
            run.updated_at = now_secs();
            run.status = if error.is_some() { "error" } else { "done" }.to_string();
            run.result = result.map(|r| r.chars().take(500).collect());
            run.error = error;
            for st in run.agents.values_mut() {
                if st.phase != "done" {
                    st.phase = "done".to_string();
                }
            }
        }
    }
    broadcast(app);
}

/// Drop the oldest finished runs once the registry exceeds `MAX_RUNS`.
fn prune(reg: &mut Registry) {
    while reg.order.len() > MAX_RUNS {
        if let Some(pos) = reg.order.iter().position(|id| {
            reg.runs
                .get(id)
                .map(|r| r.status != "running")
                .unwrap_or(true)
        }) {
            let id = reg.order.remove(pos);
            reg.runs.remove(&id);
        } else {
            break;
        }
    }
}
