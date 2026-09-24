pub mod canonical;
pub mod classifier;
pub mod commands;
pub mod engine;
pub mod errors;
pub mod job_manager;
pub mod model_manager;
pub mod paddle;
pub mod protocol;
pub mod storage;
pub mod types;

use std::fs;
use std::sync::Arc;

use tauri::{AppHandle, Manager};

use crate::db::Db;

use engine::OcrEngine;
use errors::ConverterResult;
use job_manager::JobManager;
use model_manager::ModelManager;
use paddle::PaddleOcrEngine;

pub struct DocumentConverterState {
    pub db: Db,
    pub model: Arc<ModelManager>,
    pub engine: Arc<dyn OcrEngine>,
    pub jobs: JobManager,
}

impl DocumentConverterState {
    pub fn initialize(app: &AppHandle, db: &Db) -> ConverterResult<Self> {
        storage::ensure_tables(db)?;
        let interrupted = storage::recover_interrupted(db)?;
        if interrupted > 0 {
            log::warn!("recovered {interrupted} interrupted document conversion jobs");
        }
        cleanup_known_temp_roots(db);
        let app_data = app.path().app_data_dir().map_err(|error| {
            errors::ConverterError::new("INITIALIZATION_FAILED", error.to_string())
        })?;
        let model = Arc::new(ModelManager::new(app.clone(), app_data.join("models"))?);
        let engine: Arc<dyn OcrEngine> = Arc::new(PaddleOcrEngine::new(app.clone()));
        let jobs = JobManager::new(app.clone(), db.clone(), model.clone(), engine.clone());
        Ok(Self {
            db: db.clone(),
            model,
            engine,
            jobs,
        })
    }

    pub fn shutdown(&self) {
        let engine = self.engine.clone();
        tauri::async_runtime::spawn(async move {
            let _ = engine.shutdown().await;
        });
    }
}

fn cleanup_known_temp_roots(db: &Db) {
    let Ok(jobs) = storage::list(db, 200) else {
        return;
    };
    let roots = jobs
        .into_iter()
        .map(|job| std::path::PathBuf::from(job.destination_root).join(".agentic-os-tmp"))
        .collect::<std::collections::HashSet<_>>();
    for root in roots {
        let Ok(metadata) = fs::symlink_metadata(&root) else {
            continue;
        };
        if metadata.is_dir() && !metadata.file_type().is_symlink() {
            if let Err(error) = fs::remove_dir_all(&root) {
                log::warn!("could not clean document converter temp root: {error}");
            }
        }
    }
}
