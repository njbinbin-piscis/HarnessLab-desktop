//! HarnessLab Desktop — Tauri v2 application library.
//!
//! The desktop shell hosts the React frontend (ported from HarnessLab) and
//! exposes the Rust core (`harnesslab-core` on `piscis-kernel`) over Tauri
//! commands/events instead of an HTTP API.

mod commands;

/// Build and run the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .manage(commands::AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::parse_agent_config,
            commands::registry_catalog,
            commands::registry_tools,
            commands::component_reference,
            commands::list_projects,
            commands::get_project,
            commands::create_project,
            commands::update_project,
            commands::delete_project,
            commands::get_layout,
            commands::save_layout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HarnessLab Desktop");
}
