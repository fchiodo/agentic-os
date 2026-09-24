use std::fs;
use std::path::{Path, PathBuf};

use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;

use crate::db::Db;
use crate::memory;

use super::canonical::safe_relative_path;
use super::storage;
use super::types::{
    AssetData, ConversionJob, CreateJobsRequest, CreateJobsResponse, DocumentConverterStatus,
    MarkdownPreview, MemoryImportRequest, ModelStatus, SelectedDocument, OCR_SIDECAR_VERSION,
    SIDECAR_PROTOCOL_VERSION,
};
use super::DocumentConverterState;

fn public_error(error: super::errors::ConverterError) -> String {
    format!("{}|{}", error.code(), error.public_message())
}

#[tauri::command]
pub fn document_converter_get_status(
    state: State<'_, DocumentConverterState>,
) -> Result<DocumentConverterStatus, String> {
    let model = state.model.status().map_err(public_error)?;
    let (active_jobs, queued_jobs) = storage::count_active(&state.db).map_err(public_error)?;
    Ok(DocumentConverterStatus {
        ready: model.checksum_valid && state.engine.is_available(),
        architecture: std::env::consts::ARCH.to_string(),
        sidecar_version: OCR_SIDECAR_VERSION.to_string(),
        protocol_version: SIDECAR_PROTOCOL_VERSION,
        engine: state.engine.id().to_string(),
        engine_version: state.engine.version(),
        model,
        active_jobs,
        queued_jobs,
        keep_warm_seconds: super::paddle::KEEP_WARM_SECONDS,
        local_only: true,
    })
}

#[tauri::command]
pub fn document_converter_get_model_status(
    state: State<'_, DocumentConverterState>,
) -> Result<ModelStatus, String> {
    state.model.status().map_err(public_error)
}

#[tauri::command]
pub async fn document_converter_install_model(
    state: State<'_, DocumentConverterState>,
) -> Result<ModelStatus, String> {
    state.model.install().await.map_err(public_error)
}

#[tauri::command]
pub fn document_converter_cancel_model_download(state: State<'_, DocumentConverterState>) {
    state.model.cancel_install();
}

#[tauri::command]
pub async fn document_converter_repair_model(
    state: State<'_, DocumentConverterState>,
) -> Result<ModelStatus, String> {
    state.model.repair().await.map_err(public_error)
}

#[tauri::command]
pub async fn document_converter_remove_model(
    state: State<'_, DocumentConverterState>,
) -> Result<ModelStatus, String> {
    let _ = state.engine.shutdown().await;
    state.model.remove().map_err(public_error)
}

#[tauri::command]
pub async fn document_converter_choose_files(
    app: AppHandle,
    state: State<'_, DocumentConverterState>,
) -> Result<Vec<SelectedDocument>, String> {
    let files = app
        .dialog()
        .file()
        .set_title("Choose documents")
        .add_filter("Documents", &["pdf", "png", "jpg", "jpeg"])
        .blocking_pick_files()
        .unwrap_or_default();
    let paths = files
        .into_iter()
        .filter_map(|file| file.into_path().ok())
        .map(|path| path.to_string_lossy().to_string())
        .collect();
    Ok(state.jobs.inspect_paths(paths).await)
}

#[tauri::command]
pub async fn document_converter_inspect_paths(
    state: State<'_, DocumentConverterState>,
    paths: Vec<String>,
) -> Result<Vec<SelectedDocument>, String> {
    Ok(state.jobs.inspect_paths(paths).await)
}

#[tauri::command]
pub async fn document_converter_choose_destination(
    app: AppHandle,
) -> Result<Option<String>, String> {
    Ok(app
        .dialog()
        .file()
        .set_title("Choose output folder")
        .blocking_pick_folder()
        .and_then(|file| file.into_path().ok())
        .map(|path| path.to_string_lossy().to_string()))
}

#[tauri::command]
pub async fn document_converter_create_jobs(
    state: State<'_, DocumentConverterState>,
    request: CreateJobsRequest,
) -> Result<CreateJobsResponse, String> {
    state.jobs.create_jobs(request).await.map_err(public_error)
}

#[tauri::command]
pub async fn document_converter_cancel_job(
    state: State<'_, DocumentConverterState>,
    job_id: String,
) -> Result<ConversionJob, String> {
    state.jobs.cancel(&job_id).await.map_err(public_error)
}

#[tauri::command]
pub async fn document_converter_cancel_all(
    state: State<'_, DocumentConverterState>,
) -> Result<(), String> {
    state.jobs.cancel_all().await.map_err(public_error)
}

#[tauri::command]
pub fn document_converter_retry_job(
    state: State<'_, DocumentConverterState>,
    job_id: String,
) -> Result<ConversionJob, String> {
    state.jobs.retry(&job_id).map_err(public_error)
}

#[tauri::command]
pub fn document_converter_get_job(
    state: State<'_, DocumentConverterState>,
    job_id: String,
) -> Result<ConversionJob, String> {
    storage::get(&state.db, &job_id).map_err(public_error)
}

#[tauri::command]
pub fn document_converter_list_jobs(
    state: State<'_, DocumentConverterState>,
    limit: Option<u32>,
) -> Result<Vec<ConversionJob>, String> {
    storage::list(&state.db, limit.unwrap_or(50)).map_err(public_error)
}

#[tauri::command]
pub fn document_converter_delete_history_entry(
    state: State<'_, DocumentConverterState>,
    job_id: String,
) -> Result<(), String> {
    storage::delete_history(&state.db, &job_id).map_err(public_error)
}

#[tauri::command]
pub fn document_converter_get_preview(
    state: State<'_, DocumentConverterState>,
    job_id: String,
) -> Result<MarkdownPreview, String> {
    let job = completed_job(&state.db, &job_id)?;
    let markdown_path = job
        .markdown_path
        .as_ref()
        .ok_or_else(|| "OUTPUT_NOT_FOUND|Markdown output is unavailable".to_string())?;
    let metadata = fs::metadata(markdown_path)
        .map_err(|_| "OUTPUT_NOT_FOUND|Markdown output is unavailable".to_string())?;
    if metadata.len() > 64 * 1024 * 1024 {
        return Err("RESOURCE_LIMIT_EXCEEDED|Markdown preview exceeds 64 MB".to_string());
    }
    let markdown = fs::read_to_string(markdown_path)
        .map_err(|_| "OUTPUT_NOT_FOUND|Markdown output is unavailable".to_string())?;
    Ok(MarkdownPreview {
        job_id: job.id,
        source_name: job.source_name,
        markdown,
        output_path: job.output_path.unwrap_or_default(),
        asset_count: job.asset_count,
    })
}

#[tauri::command]
pub fn document_converter_read_asset(
    state: State<'_, DocumentConverterState>,
    job_id: String,
    relative_path: String,
) -> Result<AssetData, String> {
    let job = completed_job(&state.db, &job_id)?;
    let output = PathBuf::from(job.output_path.unwrap_or_default());
    let relative = safe_relative_path(&relative_path).map_err(public_error)?;
    if relative.components().next().and_then(|value| match value {
        std::path::Component::Normal(value) => value.to_str(),
        _ => None,
    }) != Some("assets")
    {
        return Err("INVALID_PATH|Only conversion assets can be previewed".to_string());
    }
    let output = output
        .canonicalize()
        .map_err(|_| "OUTPUT_NOT_FOUND|Output package is unavailable".to_string())?;
    let raw_path = output.join(relative);
    if fs::symlink_metadata(&raw_path)
        .map_err(|_| "OUTPUT_NOT_FOUND|Asset is unavailable".to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("INVALID_PATH|Asset symlinks cannot be previewed".to_string());
    }
    let path = raw_path
        .canonicalize()
        .map_err(|_| "OUTPUT_NOT_FOUND|Asset is unavailable".to_string())?;
    if !path.starts_with(&output) {
        return Err("INVALID_PATH|Asset path is outside the output package".to_string());
    }
    let metadata =
        fs::metadata(&path).map_err(|_| "OUTPUT_NOT_FOUND|Asset is unavailable".to_string())?;
    if metadata.len() > 40 * 1024 * 1024 {
        return Err("RESOURCE_LIMIT_EXCEEDED|Asset exceeds preview limit".to_string());
    }
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let mime_type = match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        _ => return Err("UNSUPPORTED_FILE|Asset type cannot be previewed".to_string()),
    };
    let bytes = fs::read(path).map_err(|_| "OUTPUT_NOT_FOUND|Asset is unavailable".to_string())?;
    Ok(AssetData {
        mime_type: mime_type.to_string(),
        base64: BASE64_STANDARD.encode(bytes),
    })
}

#[tauri::command]
pub fn document_converter_open_output(
    state: State<'_, DocumentConverterState>,
    job_id: String,
    reveal_markdown: Option<bool>,
) -> Result<(), String> {
    let job = completed_job(&state.db, &job_id)?;
    let target = if reveal_markdown.unwrap_or(false) {
        job.markdown_path
    } else {
        job.output_path
    }
    .ok_or_else(|| "OUTPUT_NOT_FOUND|Output package is unavailable".to_string())?;
    let mut command = std::process::Command::new("/usr/bin/open");
    if reveal_markdown.unwrap_or(false) {
        command.arg("-R");
    }
    command.arg(&target);
    command
        .spawn()
        .map_err(|_| "OUTPUT_OPEN_FAILED|Finder could not open the output".to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn document_converter_import_to_memory(
    app: AppHandle,
    state: State<'_, DocumentConverterState>,
    request: MemoryImportRequest,
) -> Result<memory::DocumentImportResult, String> {
    let job = completed_job(&state.db, &request.job_id)?;
    let markdown_path = job
        .markdown_path
        .ok_or_else(|| "OUTPUT_NOT_FOUND|Markdown output is unavailable".to_string())?;
    let content = fs::read_to_string(&markdown_path)
        .map_err(|_| "OUTPUT_NOT_FOUND|Markdown output is unavailable".to_string())?;
    let provenance = format!(
        "<!-- agentic-os-provenance: source={}, source-sha256={}, fingerprint={}, engine={}, engine-version={}, model-version={}, converted-at={} -->\n\n",
        job.source_name,
        job.source_hash,
        job.fingerprint,
        job.engine,
        job.engine_version,
        job.model_version,
        job.completed_at.as_deref().unwrap_or("unknown")
    );
    let import_request = memory::DocumentImportRequest {
        domain: request.domain,
        input_kind: "text".to_string(),
        title: job.source_name,
        content: Some(format!("{provenance}{content}")),
        content_encoding: None,
        mime_type: Some("text/markdown".to_string()),
        source_url: None,
        file_name: Some(
            Path::new(&markdown_path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("document.md")
                .to_string(),
        ),
    };
    memory::importer::import_document_with_app(&state.db, &import_request, &app)
        .await
        .map_err(|error| error.to_string())
}

fn completed_job(db: &Db, id: &str) -> Result<ConversionJob, String> {
    let job = storage::get(db, id).map_err(public_error)?;
    if job.status != "completed" {
        return Err("INVALID_JOB_STATE|Conversion is not complete".to_string());
    }
    Ok(job)
}
