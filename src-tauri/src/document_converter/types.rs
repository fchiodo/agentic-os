use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

pub const OUTPUT_SCHEMA_VERSION: u32 = 1;
pub const SIDECAR_PROTOCOL_VERSION: u32 = 1;
pub const OCR_ENGINE_ID: &str = "paddleocr-vl";
pub const OCR_SIDECAR_VERSION: &str = "0.2.1";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionOptions {
    #[serde(default = "default_processing_mode")]
    pub processing_mode: String,
    #[serde(default = "default_max_tokens")]
    pub max_tokens_per_page: u32,
    #[serde(default)]
    pub preserve_page_images: bool,
}

fn default_processing_mode() -> String {
    "automatic".to_string()
}

fn default_max_tokens() -> u32 {
    2048
}

impl Default for ConversionOptions {
    fn default() -> Self {
        Self {
            processing_mode: default_processing_mode(),
            max_tokens_per_page: default_max_tokens(),
            preserve_page_images: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJobsRequest {
    pub input_paths: Vec<String>,
    pub destination_root: Option<String>,
    #[serde(default)]
    pub options: ConversionOptions,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectedDocument {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    pub page_count: Option<u32>,
    pub supported: bool,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RejectedDocument {
    pub path: String,
    pub name: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateJobsResponse {
    pub jobs: Vec<ConversionJob>,
    pub rejected: Vec<RejectedDocument>,
    pub duplicates: Vec<DuplicateConversion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateConversion {
    pub source_path: String,
    pub previous_job: ConversionJob,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionJob {
    pub id: String,
    pub source_name: String,
    pub source_path: String,
    pub source_hash: String,
    pub source_size_bytes: u64,
    pub destination_root: String,
    pub output_path: Option<String>,
    pub markdown_path: Option<String>,
    pub json_path: Option<String>,
    pub engine: String,
    pub engine_version: String,
    pub model_version: String,
    pub status: String,
    pub stage: String,
    pub stage_label: Option<String>,
    pub total_pages: Option<u32>,
    pub processed_pages: u32,
    pub pages_digital: u32,
    pub pages_ocr: u32,
    pub asset_count: u32,
    pub processing_mode: String,
    pub options: ConversionOptions,
    pub fingerprint: String,
    pub warnings: Vec<String>,
    pub created_at: String,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionProgress {
    pub job_id: String,
    pub source_name: String,
    pub status: String,
    pub stage: String,
    pub label: String,
    pub page: Option<u32>,
    pub total_pages: Option<u32>,
    pub percent: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversionFailure {
    pub job_id: String,
    pub source_name: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub state: String,
    pub installed: bool,
    pub checksum_valid: bool,
    pub download_size_bytes: u64,
    pub installed_size_bytes: u64,
    pub installed_path: Option<String>,
    pub architecture: String,
    pub license: String,
    pub error_code: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadProgress {
    pub model_id: String,
    pub stage: String,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub percent: f64,
    pub current_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentConverterStatus {
    pub ready: bool,
    pub architecture: String,
    pub sidecar_version: String,
    pub protocol_version: u32,
    pub engine: String,
    pub engine_version: String,
    pub model: ModelStatus,
    pub active_jobs: u32,
    pub queued_jobs: u32,
    pub keep_warm_seconds: u64,
    pub local_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarkdownPreview {
    pub job_id: String,
    pub source_name: String,
    pub markdown: String,
    pub output_path: String,
    pub asset_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetData {
    pub mime_type: String,
    pub base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryImportRequest {
    pub job_id: String,
    pub domain: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalBlock {
    pub id: String,
    #[serde(rename = "type")]
    pub block_type: String,
    pub page: u32,
    pub order: u32,
    pub text: Option<String>,
    pub level: Option<u8>,
    pub bbox: Option<BoundingBox>,
    pub confidence: Option<f64>,
    pub asset_ref: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalPage {
    pub number: u32,
    pub source_mode: String,
    pub rotation_correction: i32,
    pub blocks: Vec<CanonicalBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceMetadata {
    pub name: String,
    pub path: String,
    pub sha256: String,
    pub size_bytes: u64,
    pub page_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessingSummary {
    pub requested_mode: String,
    pub actual_mode: String,
    pub pages_total: u32,
    pub pages_digital: u32,
    pub pages_ocr: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CanonicalDocument {
    pub schema_version: u32,
    pub agentic_os_version: String,
    pub conversion_timestamp: String,
    pub conversion_fingerprint: String,
    pub source: SourceMetadata,
    pub engine: String,
    pub engine_version: String,
    pub model_version: String,
    pub processing: ProcessingSummary,
    pub pages: Vec<CanonicalPage>,
    pub warnings: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub metadata: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct RawPage {
    pub page_number: u32,
    pub source_mode: String,
    pub text: String,
    pub confidence: Option<f64>,
    pub rotation_correction: i32,
    pub rendered_asset_path: Option<String>,
}

#[derive(Debug, Clone)]
pub struct EngineConversion {
    pub pages: Vec<RawPage>,
    pub total_pages: u32,
    pub pages_digital: u32,
    pub pages_ocr: u32,
    pub processing_mode: String,
    pub warnings: Vec<String>,
}
