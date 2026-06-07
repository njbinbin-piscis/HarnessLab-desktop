//! Tauri commands — the desktop IPC surface that replaces HarnessLab's FastAPI
//! REST endpoints. P0 ships a minimal set proving the frontend ↔ Rust core
//! link; later phases add projects / ops / registry / eval / factory / design.

use serde::Serialize;

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

/// Smoke command proving the `harnesslab-core` link: parse a YAML agent config
/// and return its agent name. Replaced by the real project/ops commands in P1+.
#[tauri::command]
pub fn parse_agent_config(yaml: String) -> Result<String, String> {
    let cfg = harnesslab_core::config::AgentConfig::from_yaml(&yaml).map_err(|e| e.to_string())?;
    Ok(cfg.agent.name)
}
