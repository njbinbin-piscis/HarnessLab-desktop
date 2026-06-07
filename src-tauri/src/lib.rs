//! HarnessLab Desktop — Tauri v2 application library.
//!
//! The desktop shell hosts the React frontend (ported from HarnessLab) and
//! exposes the Rust core (`harnesslab-core` on `piscis-kernel`) over Tauri
//! commands/events instead of an HTTP API.

mod commands;

use harnesslab_core::store::Store;
use tauri::Manager;

/// Build and run the Tauri application.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .setup(|app| {
            // Persist projects/layout in the per-user app data dir. Fall back to
            // an in-memory store if the directory or DB cannot be opened.
            let store = app
                .path()
                .app_data_dir()
                .ok()
                .and_then(|dir| {
                    std::fs::create_dir_all(&dir).ok()?;
                    Store::open(dir.join("harnesslab.db")).ok()
                })
                .unwrap_or_else(Store::in_memory);
            let _ = store.seed_if_empty();
            app.manage(commands::AppState::new(store));
            Ok(())
        })
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
            commands::apply_ops,
            commands::run_project,
            commands::list_canvas_runs,
            commands::session_tree,
            commands::languages,
            commands::generate,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HarnessLab Desktop");
}
