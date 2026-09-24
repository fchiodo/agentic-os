use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::db::Db;

use super::canonical::{
    build_document, next_available_output, preflight_destination, sanitize_stem,
    write_document_files,
};
use super::classifier::{self, Classification, Inspection};
use super::engine::{EngineRequest, OcrEngine, ProgressReporter};
use super::errors::{ConverterError, ConverterResult};
use super::model_manager::ModelManager;
use super::storage;
use super::types::{
    ConversionFailure, ConversionJob, ConversionOptions, ConversionProgress, CreateJobsRequest,
    CreateJobsResponse, DuplicateConversion, EngineConversion, RawPage, RejectedDocument,
    SelectedDocument, SourceMetadata, OUTPUT_SCHEMA_VERSION,
};

#[cfg(test)]
use super::types::{OCR_ENGINE_ID, OCR_SIDECAR_VERSION};

const PROGRESS_EVENT: &str = "document-converter:conversion-progress";
const COMPLETED_EVENT: &str = "document-converter:conversion-completed";
const FAILED_EVENT: &str = "document-converter:conversion-failed";
const MAX_SOURCE_BYTES: u64 = 4 * 1024 * 1024 * 1024;

#[derive(Clone)]
pub struct JobManager {
    app: AppHandle,
    db: Db,
    model: Arc<ModelManager>,
    engine: Arc<dyn OcrEngine>,
    queue: mpsc::UnboundedSender<String>,
    cancelled: Arc<Mutex<HashSet<String>>>,
    active: Arc<Mutex<Option<String>>>,
}

impl JobManager {
    pub fn new(
        app: AppHandle,
        db: Db,
        model: Arc<ModelManager>,
        engine: Arc<dyn OcrEngine>,
    ) -> Self {
        let (queue, receiver) = mpsc::unbounded_channel();
        let manager = Self {
            app,
            db,
            model,
            engine,
            queue,
            cancelled: Arc::new(Mutex::new(HashSet::new())),
            active: Arc::new(Mutex::new(None)),
        };
        let worker = manager.clone();
        tauri::async_runtime::spawn(async move { worker.worker(receiver).await });
        manager
    }

    pub async fn inspect_paths(&self, paths: Vec<String>) -> Vec<SelectedDocument> {
        let mut selected = Vec::with_capacity(paths.len());
        for raw in paths {
            let path = PathBuf::from(&raw);
            let name = path
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("document")
                .to_string();
            let metadata = fs::metadata(&path);
            let result = match metadata {
                Ok(metadata) if metadata.is_file() && metadata.len() <= MAX_SOURCE_BYTES => {
                    match classifier::inspect(&path) {
                        Ok(inspection) => SelectedDocument {
                            path: raw,
                            name,
                            size_bytes: metadata.len(),
                            page_count: Some(inspection.page_count),
                            supported: true,
                            error_code: None,
                            error_message: None,
                        },
                        Err(error) => SelectedDocument {
                            path: raw,
                            name,
                            size_bytes: metadata.len(),
                            page_count: None,
                            supported: false,
                            error_code: Some(error.code().to_string()),
                            error_message: Some(error.public_message()),
                        },
                    }
                }
                Ok(metadata) if metadata.len() > MAX_SOURCE_BYTES => SelectedDocument {
                    path: raw,
                    name,
                    size_bytes: metadata.len(),
                    page_count: None,
                    supported: false,
                    error_code: Some("RESOURCE_LIMIT_EXCEEDED".to_string()),
                    error_message: Some("File exceeds the 4 GB safety limit".to_string()),
                },
                _ => SelectedDocument {
                    path: raw,
                    name,
                    size_bytes: 0,
                    page_count: None,
                    supported: false,
                    error_code: Some("UNSUPPORTED_FILE".to_string()),
                    error_message: Some("File cannot be read".to_string()),
                },
            };
            selected.push(result);
        }
        selected
    }

    pub async fn create_jobs(
        &self,
        request: CreateJobsRequest,
    ) -> ConverterResult<CreateJobsResponse> {
        validate_options(&request.options)?;
        if request.input_paths.is_empty() {
            return Err(ConverterError::new(
                "INVALID_REQUEST",
                "Choose at least one document",
            ));
        }
        let destination = destination_root(request.destination_root.as_deref())?;
        preflight_destination(&destination)?;
        let mut jobs = Vec::new();
        let mut rejected = Vec::new();
        let mut duplicates = Vec::new();

        for raw_path in request.input_paths.into_iter().take(100) {
            match self.prepare_job(&raw_path, &destination, &request.options) {
                Ok((job, duplicate)) => {
                    storage::insert(&self.db, &job)?;
                    if let Some(previous_job) = duplicate {
                        duplicates.push(DuplicateConversion {
                            source_path: raw_path,
                            previous_job,
                        });
                    }
                    self.queue.send(job.id.clone()).map_err(|_| {
                        ConverterError::new("JOB_QUEUE_FAILED", "Conversion queue is unavailable")
                    })?;
                    jobs.push(job);
                }
                Err(error) => {
                    let name = Path::new(&raw_path)
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("document")
                        .to_string();
                    rejected.push(RejectedDocument {
                        path: raw_path,
                        name,
                        code: error.code().to_string(),
                        message: error.public_message(),
                    });
                }
            }
        }
        Ok(CreateJobsResponse {
            jobs,
            rejected,
            duplicates,
        })
    }

    pub async fn cancel(&self, job_id: &str) -> ConverterResult<ConversionJob> {
        let mut job = storage::get(&self.db, job_id)?;
        if matches!(job.status.as_str(), "completed" | "failed" | "cancelled") {
            return Ok(job);
        }
        self.cancelled
            .lock()
            .expect("cancel set mutex poisoned")
            .insert(job_id.to_string());
        let active = self
            .active
            .lock()
            .expect("active job mutex poisoned")
            .as_deref()
            == Some(job_id);
        if active {
            self.engine.cancel(job_id).await?;
        } else {
            mark_cancelled(&mut job);
            storage::save(&self.db, &job)?;
            self.emit_failure(&job, "CONVERSION_CANCELLED", "Conversion was cancelled");
        }
        storage::get(&self.db, job_id)
    }

    pub async fn cancel_all(&self) -> ConverterResult<()> {
        for job in storage::list(&self.db, 200)? {
            if matches!(
                job.status.as_str(),
                "queued" | "preparing" | "rendering" | "ocr" | "reconstructing" | "writing"
            ) {
                let _ = self.cancel(&job.id).await;
            }
        }
        Ok(())
    }

    pub fn retry(&self, id: &str) -> ConverterResult<ConversionJob> {
        let mut job = storage::get(&self.db, id)?;
        if !matches!(job.status.as_str(), "failed" | "cancelled") {
            return Err(ConverterError::new(
                "INVALID_JOB_STATE",
                "Only failed or cancelled conversions can be retried",
            ));
        }
        job.id = Uuid::new_v4().to_string();
        job.status = "queued".to_string();
        job.stage = "queued".to_string();
        job.stage_label = Some("Waiting to start".to_string());
        job.output_path = None;
        job.markdown_path = None;
        job.json_path = None;
        job.processed_pages = 0;
        job.pages_digital = 0;
        job.pages_ocr = 0;
        job.asset_count = 0;
        job.created_at = chrono::Utc::now().to_rfc3339();
        job.started_at = None;
        job.completed_at = None;
        job.error_code = None;
        job.error_message = None;
        storage::insert(&self.db, &job)?;
        self.queue.send(job.id.clone()).map_err(|_| {
            ConverterError::new("JOB_QUEUE_FAILED", "Conversion queue is unavailable")
        })?;
        Ok(job)
    }

    fn prepare_job(
        &self,
        raw_path: &str,
        destination: &Path,
        options: &ConversionOptions,
    ) -> ConverterResult<(ConversionJob, Option<ConversionJob>)> {
        let path = fs::canonicalize(raw_path)
            .map_err(|_| ConverterError::new("UNSUPPORTED_FILE", "Source file cannot be read"))?;
        let metadata = fs::metadata(&path)?;
        if !metadata.is_file() || metadata.len() > MAX_SOURCE_BYTES {
            return Err(ConverterError::new(
                "RESOURCE_LIMIT_EXCEEDED",
                "Source file exceeds the safety limit",
            ));
        }
        let inspection = classifier::inspect(&path)?;
        let source_hash = classifier::hash_file(&path)?;
        let requires_ocr = route_requires_ocr(&inspection, &options.processing_mode);
        let definition = self.model.definition()?;
        let (engine, engine_version, model_version) = if requires_ocr {
            (
                self.engine.id().to_string(),
                self.engine.version(),
                definition.version.clone(),
            )
        } else {
            (
                "pdf-extract".to_string(),
                "0.8.2".to_string(),
                "none".to_string(),
            )
        };
        let fingerprint = conversion_fingerprint(
            &source_hash,
            &engine,
            &engine_version,
            &model_version,
            options,
        )?;
        let duplicate = storage::find_completed_fingerprint(&self.db, &fingerprint)?;
        let source_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("document")
            .to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let job = ConversionJob {
            id: Uuid::new_v4().to_string(),
            source_name,
            source_path: path.to_string_lossy().to_string(),
            source_hash,
            source_size_bytes: metadata.len(),
            destination_root: destination.to_string_lossy().to_string(),
            output_path: None,
            markdown_path: None,
            json_path: None,
            engine,
            engine_version,
            model_version,
            status: "queued".to_string(),
            stage: "queued".to_string(),
            stage_label: Some("Waiting to start".to_string()),
            total_pages: Some(inspection.page_count),
            processed_pages: 0,
            pages_digital: 0,
            pages_ocr: 0,
            asset_count: 0,
            processing_mode: options.processing_mode.clone(),
            options: options.clone(),
            fingerprint,
            warnings: inspection.warnings,
            created_at: now,
            started_at: None,
            completed_at: None,
            error_code: None,
            error_message: None,
        };
        Ok((job, duplicate))
    }

    async fn worker(&self, mut receiver: mpsc::UnboundedReceiver<String>) {
        while let Some(job_id) = receiver.recv().await {
            if self.is_cancelled(&job_id) {
                continue;
            }
            *self.active.lock().expect("active job mutex poisoned") = Some(job_id.clone());
            let result = self.process_job(&job_id).await;
            *self.active.lock().expect("active job mutex poisoned") = None;
            if let Err(error) = result {
                let mut cancelled = error.code() == "CONVERSION_CANCELLED";
                if let Ok(mut job) = storage::get(&self.db, &job_id) {
                    cancelled = cancelled || self.is_cancelled(&job_id);
                    if cancelled {
                        mark_cancelled(&mut job);
                    } else {
                        job.status = "failed".to_string();
                        job.stage = "failed".to_string();
                        job.stage_label = Some("Conversion failed".to_string());
                        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
                        job.error_code = Some(error.code().to_string());
                        job.error_message = Some(error.public_message());
                    }
                    let _ = storage::save(&self.db, &job);
                    self.emit_failure(&job, error.code(), &error.public_message());
                }
                if cancelled {
                    log::info!("document conversion {job_id} cancelled");
                } else {
                    log::error!("document conversion {job_id} failed: {error}");
                }
            }
            self.cancelled
                .lock()
                .expect("cancel set mutex poisoned")
                .remove(&job_id);
        }
    }

    async fn process_job(&self, job_id: &str) -> ConverterResult<()> {
        let mut job = storage::get(&self.db, job_id)?;
        update_stage(
            &self.db,
            &self.app,
            &mut job,
            "preparing",
            "Inspecting document",
            None,
            None,
        )?;
        let source_path = PathBuf::from(&job.source_path);
        let inspection = classifier::inspect(&source_path)?;
        job.total_pages = Some(inspection.page_count);
        job.warnings.extend(inspection.warnings.clone());
        storage::save(&self.db, &job)?;

        if self.is_cancelled(job_id) {
            return Err(ConverterError::new(
                "CONVERSION_CANCELLED",
                "Conversion was cancelled",
            ));
        }
        let destination = PathBuf::from(&job.destination_root);
        preflight_destination(&destination)?;
        let temp_root = destination.join(".agentic-os-tmp");
        fs::create_dir_all(&temp_root)?;
        let temp_package = temp_root.join(job_id);
        if temp_package.exists() {
            safe_remove_temp(&temp_root, &temp_package)?;
        }
        fs::create_dir(&temp_package)?;
        fs::create_dir(temp_package.join("assets"))?;
        let working = temp_package.join(".work");
        fs::create_dir(&working)?;

        let result = self.extract(&mut job, inspection, &working).await;
        let conversion = match result {
            Ok(conversion) => conversion,
            Err(error) => {
                let _ = safe_remove_temp(&temp_root, &temp_package);
                return Err(error);
            }
        };
        if self.is_cancelled(job_id) {
            let _ = safe_remove_temp(&temp_root, &temp_package);
            return Err(ConverterError::new(
                "CONVERSION_CANCELLED",
                "Conversion was cancelled",
            ));
        }

        let total_pages = job.total_pages;
        update_stage(
            &self.db,
            &self.app,
            &mut job,
            "reconstructing",
            "Reconstructing document",
            None,
            total_pages,
        )?;
        let mut conversion = conversion;
        job.asset_count = materialize_assets(
            &temp_package,
            &working,
            &mut conversion,
            job.options.preserve_page_images,
        )?;
        job.pages_digital = conversion.pages_digital;
        job.pages_ocr = conversion.pages_ocr;
        job.processed_pages = conversion.total_pages;
        job.processing_mode = conversion.processing_mode.clone();
        job.warnings.extend(conversion.warnings.clone());

        let source = SourceMetadata {
            name: job.source_name.clone(),
            path: job.source_path.clone(),
            sha256: job.source_hash.clone(),
            size_bytes: job.source_size_bytes,
            page_count: conversion.total_pages,
        };
        let canonical = build_document(
            source,
            job.fingerprint.clone(),
            &job.engine,
            &job.engine_version,
            &job.model_version,
            &job.options.processing_mode,
            conversion,
        );
        let total_pages = job.total_pages;
        update_stage(
            &self.db,
            &self.app,
            &mut job,
            "writing",
            "Writing Markdown and JSON",
            None,
            total_pages,
        )?;
        let _ = fs::remove_dir_all(&working);
        let stem = sanitize_stem(&job.source_name);
        let (markdown_temp, json_temp) = write_document_files(&temp_package, &stem, &canonical)?;
        if !markdown_temp.is_file() || !json_temp.is_file() {
            let _ = safe_remove_temp(&temp_root, &temp_package);
            return Err(ConverterError::new(
                "OUTPUT_WRITE_FAILED",
                "Conversion output could not be validated",
            ));
        }
        let output = next_available_output(&destination, &stem)?;
        fs::rename(&temp_package, &output)?;
        let _ = fs::remove_dir(&temp_root);

        job.status = "completed".to_string();
        job.stage = "completed".to_string();
        job.stage_label = Some("Conversion complete".to_string());
        job.output_path = Some(output.to_string_lossy().to_string());
        job.markdown_path = Some(
            output
                .join(format!("{stem}.md"))
                .to_string_lossy()
                .to_string(),
        );
        job.json_path = Some(output.join("document.json").to_string_lossy().to_string());
        job.completed_at = Some(chrono::Utc::now().to_rfc3339());
        storage::save(&self.db, &job)?;
        let _ = self.app.emit(COMPLETED_EVENT, &job);
        log::info!(
            "document conversion completed: job={}, pages={}, digital={}, ocr={}, assets={}",
            job.id,
            job.processed_pages,
            job.pages_digital,
            job.pages_ocr,
            job.asset_count
        );
        Ok(())
    }

    async fn extract(
        &self,
        job: &mut ConversionJob,
        inspection: Inspection,
        working: &Path,
    ) -> ConverterResult<EngineConversion> {
        let requires_ocr = route_requires_ocr(&inspection, &job.options.processing_mode);
        if !requires_ocr {
            update_stage(
                &self.db,
                &self.app,
                job,
                "reconstructing",
                "Extracting embedded text",
                Some(inspection.page_count),
                Some(inspection.page_count),
            )?;
            return digital_conversion(inspection, &job.options.processing_mode);
        }
        let model = self.model.status()?;
        if !model.checksum_valid {
            return Err(ConverterError::new(
                "MODEL_NOT_INSTALLED",
                "Install or repair Document AI before using OCR",
            ));
        }
        let db = self.db.clone();
        let app = self.app.clone();
        let progress: ProgressReporter = Arc::new(move |event| {
            if let Ok(mut stored) = storage::get(&db, &event.job_id) {
                stored.stage = event.stage.clone();
                // Sidecar stages are intentionally richer than the persisted job state
                // machine. Keep those labels for UX, but map them onto stable states so
                // a final digital page in a hybrid PDF cannot strand the job in an
                // unrecognised `extracting` state.
                stored.status = match event.stage.as_str() {
                    "rendering" | "extracting" => "rendering",
                    "ocr" | "loading-model" => "ocr",
                    _ => stored.status.as_str(),
                }
                .to_string();
                stored.stage_label = Some(event.label.clone());
                stored.total_pages = event.total_pages.or(stored.total_pages);
                stored.processed_pages = event.page.unwrap_or(stored.processed_pages);
                let _ = storage::save(&db, &stored);
            }
            let _ = app.emit(PROGRESS_EVENT, event);
        });
        let request = EngineRequest {
            job_id: job.id.clone(),
            source_name: job.source_name.clone(),
            input_path: PathBuf::from(&job.source_path),
            working_directory: working.to_path_buf(),
            model_path: self.model.installed_path()?,
            options: job.options.clone(),
        };
        self.engine.convert(request, progress).await
    }

    fn is_cancelled(&self, id: &str) -> bool {
        self.cancelled
            .lock()
            .expect("cancel set mutex poisoned")
            .contains(id)
    }

    fn emit_failure(&self, job: &ConversionJob, code: &str, message: &str) {
        let _ = self.app.emit(
            FAILED_EVENT,
            ConversionFailure {
                job_id: job.id.clone(),
                source_name: job.source_name.clone(),
                code: code.to_string(),
                message: message.to_string(),
            },
        );
    }
}

fn update_stage(
    db: &Db,
    app: &AppHandle,
    job: &mut ConversionJob,
    stage: &str,
    label: &str,
    page: Option<u32>,
    total: Option<u32>,
) -> ConverterResult<()> {
    if !storage::can_transition(&job.status, stage) {
        return Err(ConverterError::new(
            "INVALID_JOB_STATE",
            "Conversion entered an invalid state",
        ));
    }
    job.status = stage.to_string();
    job.stage = stage.to_string();
    job.stage_label = Some(label.to_string());
    if job.started_at.is_none() {
        job.started_at = Some(chrono::Utc::now().to_rfc3339());
    }
    job.total_pages = total.or(job.total_pages);
    job.processed_pages = page.unwrap_or(job.processed_pages);
    storage::save(db, job)?;
    let percent = match (page, total) {
        (Some(page), Some(total)) if total > 0 => Some(page as f64 / total as f64 * 100.0),
        _ => None,
    };
    let _ = app.emit(
        PROGRESS_EVENT,
        ConversionProgress {
            job_id: job.id.clone(),
            source_name: job.source_name.clone(),
            status: job.status.clone(),
            stage: stage.to_string(),
            label: label.to_string(),
            page,
            total_pages: total,
            percent,
        },
    );
    Ok(())
}

fn mark_cancelled(job: &mut ConversionJob) {
    job.status = "cancelled".to_string();
    job.stage = "cancelled".to_string();
    job.stage_label = Some("Conversion cancelled".to_string());
    job.completed_at = Some(chrono::Utc::now().to_rfc3339());
    job.error_code = Some("CONVERSION_CANCELLED".to_string());
    job.error_message = Some("Conversion was cancelled".to_string());
}

fn route_requires_ocr(inspection: &Inspection, mode: &str) -> bool {
    match mode {
        "force-ocr" => true,
        "digital-only" => false,
        _ => !matches!(inspection.classification, Classification::DigitalPdf),
    }
}

fn digital_conversion(
    inspection: Inspection,
    requested_mode: &str,
) -> ConverterResult<EngineConversion> {
    if matches!(inspection.classification, Classification::Image) {
        return Err(ConverterError::new(
            "UNSUPPORTED_FILE",
            "Digital text only cannot process image files",
        ));
    }
    let text = inspection.extracted_text.unwrap_or_default();
    let mut chunks = text.split('\u{c}').map(str::trim).collect::<Vec<_>>();
    while chunks.last().is_some_and(|value| value.is_empty()) {
        chunks.pop();
    }
    let mut pages = Vec::with_capacity(inspection.page_count as usize);
    for page_number in 1..=inspection.page_count {
        let value = chunks
            .get((page_number - 1) as usize)
            .copied()
            .unwrap_or_default()
            .to_string();
        pages.push(RawPage {
            page_number,
            source_mode: "digital".to_string(),
            text: value,
            confidence: None,
            rotation_correction: 0,
            rendered_asset_path: None,
        });
    }
    Ok(EngineConversion {
        pages,
        total_pages: inspection.page_count,
        pages_digital: inspection.page_count,
        pages_ocr: 0,
        processing_mode: if requested_mode == "digital-only" {
            "digital-only"
        } else {
            "digital"
        }
        .to_string(),
        warnings: inspection.warnings,
    })
}

fn materialize_assets(
    package: &Path,
    working: &Path,
    conversion: &mut EngineConversion,
    preserve_page_images: bool,
) -> ConverterResult<u32> {
    let assets = package.join("assets");
    let working = working.canonicalize()?;
    let mut count = 0u32;
    for page in &mut conversion.pages {
        let Some(rendered) = page.rendered_asset_path.take() else {
            continue;
        };
        if !preserve_page_images {
            continue;
        }
        let raw_source = PathBuf::from(rendered);
        if fs::symlink_metadata(&raw_source)?.file_type().is_symlink() {
            return Err(ConverterError::new(
                "SIDECAR_PROTOCOL_ERROR",
                "Document AI returned a symlink asset",
            ));
        }
        let source = raw_source.canonicalize()?;
        if !source.starts_with(&working) {
            return Err(ConverterError::new(
                "SIDECAR_PROTOCOL_ERROR",
                "Document AI returned an unsafe asset path",
            ));
        }
        let name = format!("page-{:03}-image-001.png", page.page_number);
        fs::copy(&source, assets.join(&name))?;
        page.rendered_asset_path = Some(format!("assets/{name}"));
        count += 1;
    }
    Ok(count)
}

fn safe_remove_temp(root: &Path, target: &Path) -> ConverterResult<()> {
    let root = root.canonicalize()?;
    let parent = target
        .parent()
        .ok_or_else(|| ConverterError::new("INVALID_PATH", "Temporary path has no parent"))?
        .canonicalize()?;
    if parent != root {
        return Err(ConverterError::new(
            "INVALID_PATH",
            "Refusing to clean a directory outside the managed temporary root",
        ));
    }
    let metadata = fs::symlink_metadata(target)?;
    if metadata.file_type().is_symlink() {
        return Err(ConverterError::new(
            "INVALID_PATH",
            "Refusing to follow a temporary symlink",
        ));
    }
    fs::remove_dir_all(target)?;
    Ok(())
}

fn destination_root(raw: Option<&str>) -> ConverterResult<PathBuf> {
    let path = match raw {
        Some(value) => PathBuf::from(value),
        None => dirs::document_dir().ok_or_else(|| {
            ConverterError::new("OUTPUT_WRITE_FAILED", "Documents directory is unavailable")
        })?,
    };
    fs::canonicalize(path).map_err(|_| {
        ConverterError::new("OUTPUT_WRITE_FAILED", "Output destination does not exist")
    })
}

fn validate_options(options: &ConversionOptions) -> ConverterResult<()> {
    if !matches!(
        options.processing_mode.as_str(),
        "automatic" | "force-ocr" | "digital-only"
    ) {
        return Err(ConverterError::new(
            "INVALID_REQUEST",
            "Processing mode is invalid",
        ));
    }
    if !(256..=8192).contains(&options.max_tokens_per_page) {
        return Err(ConverterError::new(
            "INVALID_REQUEST",
            "Page token limit is invalid",
        ));
    }
    Ok(())
}

fn conversion_fingerprint(
    source_hash: &str,
    engine: &str,
    engine_version: &str,
    model_version: &str,
    options: &ConversionOptions,
) -> ConverterResult<String> {
    #[derive(Serialize)]
    #[serde(rename_all = "camelCase")]
    struct Fingerprint<'a> {
        source_hash: &'a str,
        engine: &'a str,
        engine_version: &'a str,
        model_version: &'a str,
        output_schema_version: u32,
        options: &'a ConversionOptions,
    }
    let encoded = serde_json::to_vec(&Fingerprint {
        source_hash,
        engine,
        engine_version,
        model_version,
        output_schema_version: OUTPUT_SCHEMA_VERSION,
        options,
    })?;
    Ok(format!("{:x}", Sha256::digest(encoded)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_changes_with_processing_mode() {
        let automatic = conversion_fingerprint(
            "hash",
            OCR_ENGINE_ID,
            OCR_SIDECAR_VERSION,
            "1.6",
            &ConversionOptions::default(),
        )
        .unwrap();
        let forced = conversion_fingerprint(
            "hash",
            OCR_ENGINE_ID,
            OCR_SIDECAR_VERSION,
            "1.6",
            &ConversionOptions {
                processing_mode: "force-ocr".to_string(),
                ..ConversionOptions::default()
            },
        )
        .unwrap();
        assert_ne!(automatic, forced);
    }
}
