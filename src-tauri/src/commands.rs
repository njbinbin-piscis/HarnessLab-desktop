//! Tauri commands — the desktop IPC surface that replaces HarnessLab's FastAPI
//! REST endpoints. P1 covers the read-only-canvas surface: the component
//! catalog, the project CRUD, and the canvas layout. Run/eval/design/market
//! land in later phases.

use std::collections::BTreeMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use harnesslab_core::factory::{self, GenerateOutput};
use harnesslab_core::project::{new_id, now_secs, Project};
use harnesslab_core::registry::{self, ComponentReference, ComponentSpec, ToolSpec};
use harnesslab_core::runner::{self, LlmCreds, NodeMetrics, UiEvent};
use harnesslab_core::store::{
    CanvasRun, Layout, OpsError, OpsResult, ProjectSummary, SessionTree, Store,
};
use serde::Serialize;
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{Manager, State};
use tokio::sync::mpsc;

/// Shared application state: the SQLite-backed project/layout store behind a mutex.
pub struct AppState {
    pub store: Mutex<Store>,
}

impl AppState {
    pub fn new(store: Store) -> Self {
        AppState {
            store: Mutex::new(store),
        }
    }
}

#[derive(Serialize)]
pub struct AppInfo {
    pub name: String,
    pub version: String,
    pub engine: String,
}

/// Basic app metadata for the frontend status bar.
#[tauri::command]
pub fn app_info() -> AppInfo {
    AppInfo {
        name: "HarnessLab".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        engine: "piscis-kernel".into(),
    }
}

// ── Registry / catalog ──────────────────────────────────────────────────────

#[tauri::command]
pub fn registry_catalog() -> BTreeMap<String, Vec<ComponentSpec>> {
    registry::catalog()
}

#[derive(Serialize)]
pub struct ToolsList {
    pub tools: Vec<ToolSpec>,
}

#[tauri::command]
pub fn registry_tools() -> ToolsList {
    ToolsList {
        tools: registry::tools(),
    }
}

#[tauri::command]
pub fn component_reference(kind: String, name: String) -> ComponentReference {
    registry::component_reference(&kind, &name)
}

// ── Projects ────────────────────────────────────────────────────────────────

#[tauri::command]
pub fn list_projects(state: State<'_, AppState>) -> Vec<ProjectSummary> {
    state.store.lock().unwrap().list_projects()
}

#[tauri::command]
pub fn get_project(state: State<'_, AppState>, id: String) -> Result<Project, String> {
    state
        .store
        .lock()
        .unwrap()
        .get_project(&id)
        .ok_or_else(|| format!("project {id} not found"))
}

#[tauri::command]
pub fn create_project(state: State<'_, AppState>, project: Project) -> Project {
    state.store.lock().unwrap().create_project(project)
}

#[tauri::command]
pub fn update_project(
    state: State<'_, AppState>,
    id: String,
    project: Project,
) -> Result<Project, String> {
    state
        .store
        .lock()
        .unwrap()
        .update_project(&id, project)
        .ok_or_else(|| format!("project {id} not found"))
}

#[tauri::command]
pub fn delete_project(state: State<'_, AppState>, id: String) -> Result<(), String> {
    if state.store.lock().unwrap().delete_project(&id) {
        Ok(())
    } else {
        Err(format!("project {id} not found"))
    }
}

// ── Layout (canvas view-state) ──────────────────────────────────────────────

#[tauri::command]
pub fn get_layout(state: State<'_, AppState>, id: String) -> Layout {
    state.store.lock().unwrap().get_layout(&id)
}

#[tauri::command]
pub fn save_layout(state: State<'_, AppState>, id: String, layout: Layout) -> Layout {
    state.store.lock().unwrap().save_layout(&id, layout)
}

// ── Edit Ops (P2) ───────────────────────────────────────────────────────────

/// Serializable error payload for `apply_ops`, carrying an HTTP-like `status`
/// so the frontend can keep its 409 (stale) / 422 (invalid) / 404 handling.
#[derive(Serialize)]
pub struct ApplyErr {
    pub status: u16,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problems: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_rev: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project: Option<Project>,
}

#[tauri::command]
pub fn apply_ops(
    state: State<'_, AppState>,
    id: String,
    ops: Vec<Value>,
    dry_run: bool,
    base_rev: Option<u64>,
) -> Result<OpsResult, ApplyErr> {
    state
        .store
        .lock()
        .unwrap()
        .apply_ops(&id, &ops, dry_run, base_rev)
        .map_err(|e| match e {
            OpsError::NotFound => ApplyErr {
                status: 404,
                message: format!("project {id} not found"),
                problems: None,
                current_rev: None,
                project: None,
            },
            OpsError::Stale {
                current_rev,
                project,
            } => ApplyErr {
                status: 409,
                message: "project was modified (stale revision)".into(),
                problems: None,
                current_rev: Some(current_rev),
                project: Some(project),
            },
            OpsError::Invalid { problems } => ApplyErr {
                status: 422,
                message: "invalid edit ops".into(),
                problems: Some(problems),
                current_rev: None,
                project: None,
            },
        })
}

// ── Run console / live execution (P3) ────────────────────────────────────────

/// Streaming messages pushed to the frontend over a Tauri IPC channel during a
/// run. Mirrors the web SSE envelope (`{type:"event"|"result"|"error", …}`).
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RunMsg {
    Event { event: UiEvent },
    Result { result: String, run: CanvasRun },
    Error { message: String },
}

#[derive(Serialize)]
pub struct RunsResponse {
    pub runs: Vec<CanvasRun>,
}

/// Run a task against the project's primary agent over `piscis-kernel`,
/// streaming translated events to `on_event` and persisting the run on success.
#[tauri::command]
pub async fn run_project(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    id: String,
    task: String,
    on_event: Channel<RunMsg>,
) -> Result<(), String> {
    let project = {
        let store = state.store.lock().unwrap();
        store.get_project(&id)
    };
    let Some(project) = project else {
        let _ = on_event.send(RunMsg::Error {
            message: format!("project {id} not found"),
        });
        return Ok(());
    };

    let creds = match LlmCreds::from_env() {
        Ok(c) => c,
        Err(e) => {
            let _ = on_event.send(RunMsg::Error { message: e });
            return Ok(());
        }
    };

    let base = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let workspace = runner::project_workspace(&base, &id);
    let _ = std::fs::create_dir_all(&workspace);

    let run_id = new_id();
    let started = now_secs();
    let cancel = Arc::new(AtomicBool::new(false));

    let (tx, mut rx) = mpsc::channel::<UiEvent>(256);
    let proj = project.clone();
    let task_run = task.clone();
    let creds_run = creds.clone();
    let ws = workspace.clone();
    let cancel_run = cancel.clone();
    let handle = tokio::spawn(async move {
        runner::run_project_task(&proj, &task_run, &creds_run, &ws, tx, cancel_run).await
    });

    let mut collected: Vec<UiEvent> = Vec::new();
    while let Some(ev) = rx.recv().await {
        collected.push(ev.clone());
        let _ = on_event.send(RunMsg::Event { event: ev });
    }

    let outcome = handle
        .await
        .map_err(|e| format!("run task join error: {e}"))?;

    match outcome {
        Ok(out) => {
            let metrics: BTreeMap<String, NodeMetrics> =
                runner::metrics_from_events(&collected).into_iter().collect();
            let run = CanvasRun {
                id: run_id,
                project_id: id.clone(),
                project_rev: project.rev,
                parent: None,
                task,
                result: out.result.clone(),
                created_at: started,
                duration_ms: ((now_secs() - started) * 1000.0).round() as u64,
                total_tokens: out.total_input_tokens + out.total_output_tokens,
                metrics,
            };
            {
                let store = state.store.lock().unwrap();
                store.save_canvas_run(&run);
            }
            let _ = on_event.send(RunMsg::Result {
                result: out.result,
                run,
            });
            Ok(())
        }
        Err(e) => {
            let _ = on_event.send(RunMsg::Error { message: e });
            Ok(())
        }
    }
}

#[tauri::command]
pub fn list_canvas_runs(
    state: State<'_, AppState>,
    id: String,
    limit: Option<u32>,
) -> RunsResponse {
    let runs = state
        .store
        .lock()
        .unwrap()
        .list_canvas_runs(&id, limit.unwrap_or(25));
    RunsResponse { runs }
}

#[tauri::command]
pub fn session_tree(state: State<'_, AppState>, id: String, limit: Option<u32>) -> SessionTree {
    state
        .store
        .lock()
        .unwrap()
        .session_tree(&id, limit.unwrap_or(200))
}

// ── Factory codegen (P4) ─────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct LanguagesResponse {
    pub languages: Vec<String>,
}

#[tauri::command]
pub fn languages() -> LanguagesResponse {
    LanguagesResponse {
        languages: factory::languages(),
    }
}

#[tauri::command]
pub fn generate(
    state: State<'_, AppState>,
    id: String,
    language: String,
    options: Option<Value>,
) -> Result<GenerateOutput, String> {
    let project = {
        let store = state.store.lock().unwrap();
        store.get_project(&id)
    }
    .ok_or_else(|| format!("project {id} not found"))?;
    factory::generate(&project, &language, &options.unwrap_or(Value::Null))
}

/// Smoke command retained from P0: parse a YAML agent config and return its
/// agent name (proves the config→harness path is linked).
#[tauri::command]
pub fn parse_agent_config(yaml: String) -> Result<String, String> {
    let cfg = harnesslab_core::config::AgentConfig::from_yaml(&yaml).map_err(|e| e.to_string())?;
    Ok(cfg.agent.name)
}
