//! HarnessLab Desktop — Tauri v2 application library.
//!
//! The desktop shell hosts the React frontend (ported from HarnessLab) and
//! exposes the Rust core (`harnesslab-core` on `harness-rt`) over Tauri
//! commands/events instead of an HTTP API.

mod commands;
mod runtime;

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
            commands::publish_skills_on_startup(&store);
            commands::register_algorithms_on_startup(&store);
            let market_root = app
                .path()
                .app_data_dir()
                .map(|d| d.join("market"))
                .unwrap_or_else(|_| std::path::PathBuf::from("market-data"));
            let market = harnesslab_core::market::MarketStore::new(market_root);
            app.manage(commands::AppState::new(store, market));
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
            commands::download_factory_zip,
            commands::design_edit,
            commands::design_project,
            commands::market_installed,
            commands::market_sample_source,
            commands::market_inspect,
            commands::market_install,
            commands::market_uninstall,
            commands::market_get_registries,
            commands::market_set_registries,
            commands::market_catalog,
            commands::list_templates,
            commands::create_from_template,
            commands::list_library_agents,
            commands::get_library_agent,
            commands::create_library_agent,
            commands::update_library_agent,
            commands::delete_library_agent,
            commands::run_library_agent,
            commands::clawhub_search,
            commands::clawhub_install,
            commands::list_skills,
            commands::uninstall_skill,
            commands::set_skill_enabled,
            commands::list_datasets,
            commands::run_eval,
            commands::get_eval_run,
            commands::list_eval_runs,
            commands::eval_leaderboard,
            commands::register_algorithm,
            commands::author_algorithm,
            commands::list_algorithms,
            commands::delete_algorithm,
            commands::run_algorithm_trial,
            commands::runtime_runs,
            commands::embedding_status,
            commands::reload_library,
            commands::run_autoloop,
        ])
        .run(tauri::generate_context!())
        .expect("error while running HarnessLab Desktop");
}
