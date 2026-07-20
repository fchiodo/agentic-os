mod approval;
mod audit;
mod commands;
mod control_models;
mod db;
mod discovery;
mod error;
mod harness;
mod models;
mod orchestrator;
mod policy;
mod snapshot;

use db::Db;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            if cfg!(debug_assertions) {
                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        .level(log::LevelFilter::Info)
                        .build(),
                )?;
            }

            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("app data directory is unavailable");
            let db_path = app_data_dir.join("agent-control.db");
            let db = Db::open(&db_path)
                .unwrap_or_else(|err| panic!("failed to open app database at {db_path:?}: {err}"));
            app.manage(db);

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_app_snapshot,
            commands::refresh_app_snapshot,
            commands::control_status,
            commands::tasks_list,
            commands::tasks_get,
            commands::tasks_events_since,
            commands::tasks_submit,
            commands::tasks_cancel,
            commands::approvals_list,
            commands::approvals_decide,
            commands::audit_runs,
            commands::audit_trace,
            commands::audit_verify_chain,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
