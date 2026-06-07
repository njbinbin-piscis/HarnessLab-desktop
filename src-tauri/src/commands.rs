//! Tauri commands — the desktop IPC surface that replaces HarnessLab's FastAPI
//! REST endpoints. P1 covers the read-only-canvas surface: the component
//! catalog, the project CRUD, and the canvas layout. Run/eval/design/market
//! land in later phases.

use std::collections::BTreeMap;
use std::sync::Mutex;

use harnesslab_core::project::Project;
use harnesslab_core::registry::{self, ComponentReference, ComponentSpec, ToolSpec};
use harnesslab_core::store::{Layout, ProjectSummary, Store};
use serde::Serialize;
use tauri::State;

/// Shared application state: the in-memory project/layout store behind a mutex.
pub struct AppState {
    pub store: Mutex<Store>,
}

impl Default for AppState {
    fn default() -> Self {
        AppState {
            store: Mutex::new(Store::seeded()),
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

/// Smoke command retained from P0: parse a YAML agent config and return its
/// agent name (proves the config→harness path is linked).
#[tauri::command]
pub fn parse_agent_config(yaml: String) -> Result<String, String> {
    let cfg = harnesslab_core::config::AgentConfig::from_yaml(&yaml).map_err(|e| e.to_string())?;
    Ok(cfg.agent.name)
}
