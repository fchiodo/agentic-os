mod audit;
mod commands;
mod control_models;
mod db;
mod discovery;
mod document_converter;
mod error;
mod harness;
mod memory;
mod models;
mod orbit;
mod snapshot;

use db::Db;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
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
            let db_path = std::env::var_os("AGENTIC_OS_DB_PATH")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|| app_data_dir.join("agent-control.db"));
            let db = Db::open(&db_path)
                .unwrap_or_else(|err| panic!("failed to open app database at {db_path:?}: {err}"));
            memory::vault::ensure_vault()
                .unwrap_or_else(|err| panic!("failed to initialize memory vault: {err}"));
            let recovery = memory::operations::recover(&db)
                .unwrap_or_else(|err| panic!("failed to recover memory operations: {err}"));
            if recovery.recovered > 0 || recovery.rolled_back > 0 || recovery.needs_attention > 0 {
                log::info!(
                    "memory recovery: {} recovered, {} rolled back, {} need attention",
                    recovery.recovered,
                    recovery.rolled_back,
                    recovery.needs_attention
                );
            }
            memory::index::reindex(&db)
                .unwrap_or_else(|err| panic!("failed to rebuild memory index: {err}"));
            app.manage(db.clone());
            app.manage(commands::AskCancellations::default());
            let document_converter = document_converter::DocumentConverterState::initialize(
                app.handle(),
                &db,
            )
            .unwrap_or_else(|err| panic!("failed to initialize Document Converter: {err}"));
            app.manage(document_converter);

            // Memory maintenance scheduler (MEMORY-SPEC §6): sweep on app
            // start, then every 24h while the app runs. Failures are logged,
            // never fatal — the manual memory_maintenance_run command stays
            // available as fallback.
            tauri::async_runtime::spawn(async move {
                loop {
                    match memory::maintenance::run_sweep(&db) {
                        Ok(result) => {
                            if result.expired > 0
                                || result.marked_stale > 0
                                || result.consolidation_proposals > 0
                                || result.deferred_expirations > 0
                            {
                                log::info!(
                                    "memory maintenance: {} expired, {} marked stale, {} consolidation proposals, {} deferred",
                                    result.expired,
                                    result.marked_stale,
                                    result.consolidation_proposals,
                                    result.deferred_expirations
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
            commands::memory_tree,
            commands::memory_read,
            commands::memory_search,
            commands::memory_ask,
            commands::memory_ask_cancel,
            commands::memory_lint,
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
            commands::memory_operations_list,
            commands::memory_retrieval_benchmark,
            commands::memory_retrieval_eval_cases_list,
            commands::memory_retrieval_eval_case_save,
            commands::memory_orbit_map,
            document_converter::commands::document_converter_get_status,
            document_converter::commands::document_converter_get_model_status,
            document_converter::commands::document_converter_install_model,
            document_converter::commands::document_converter_cancel_model_download,
            document_converter::commands::document_converter_repair_model,
            document_converter::commands::document_converter_remove_model,
            document_converter::commands::document_converter_choose_files,
            document_converter::commands::document_converter_inspect_paths,
            document_converter::commands::document_converter_choose_destination,
            document_converter::commands::document_converter_create_jobs,
            document_converter::commands::document_converter_cancel_job,
            document_converter::commands::document_converter_cancel_all,
            document_converter::commands::document_converter_retry_job,
            document_converter::commands::document_converter_get_job,
            document_converter::commands::document_converter_list_jobs,
            document_converter::commands::document_converter_delete_history_entry,
            document_converter::commands::document_converter_get_preview,
            document_converter::commands::document_converter_read_asset,
            document_converter::commands::document_converter_open_output,
            document_converter::commands::document_converter_import_to_memory,
        ])
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window.state::<document_converter::DocumentConverterState>().shutdown();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
