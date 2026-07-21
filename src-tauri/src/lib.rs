mod approval;
mod audit;
mod commands;
mod control_models;
mod db;
mod discovery;
mod error;
mod harness;
mod memory;
mod models;
mod orchestrator;
mod policy;
mod snapshot;

use db::Db;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
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
            memory::vault::ensure_vault()
                .unwrap_or_else(|err| panic!("failed to initialize memory vault: {err}"));
            memory::index::reindex(&db)
                .unwrap_or_else(|err| panic!("failed to rebuild memory index: {err}"));
            app.manage(db.clone());

            // Memory maintenance scheduler (MEMORY-SPEC §6): sweep on app
            // start, then every 24h while the app runs. Failures are logged,
            // never fatal — the manual memory_maintenance_run command stays
            // available as fallback.
            tauri::async_runtime::spawn(async move {
                loop {
                    match memory::maintenance::run_sweep(&db) {
                        Ok(result) => {
                            if result.expired > 0 || result.marked_stale > 0 {
                                log::info!(
                                    "memory maintenance: {} expired, {} marked stale",
                                    result.expired,
                                    result.marked_stale
                                );
                            }
                        }
                        Err(err) => log::error!("memory maintenance sweep failed: {err}"),
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
                }
            });

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
            commands::memory_tree,
            commands::memory_read,
            commands::memory_search,
            commands::memory_ask,
            commands::memory_answer_feedback,
            commands::memory_save_manual,
            commands::memory_ingest,
            commands::memory_import_document,
            commands::memory_document_imports_list,
            commands::memory_document_source_read,
            commands::memory_proposals_list,
            commands::memory_proposals_decide,
            commands::memory_confirm,
            commands::memory_reindex,
            commands::memory_maintenance_run,
            commands::skills_distill,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
