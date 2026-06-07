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
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::parse_agent_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HarnessLab Desktop");
}
