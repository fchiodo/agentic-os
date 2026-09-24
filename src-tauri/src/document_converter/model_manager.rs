use std::ffi::CString;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex as StdMutex};

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter};
use tokio::sync::Mutex;
use uuid::Uuid;

use super::errors::{ConverterError, ConverterResult};
use super::types::{
    ModelDownloadProgress, ModelStatus, OCR_ENGINE_ID, OCR_SIDECAR_VERSION,
    SIDECAR_PROTOCOL_VERSION,
};

const MODEL_MANIFEST_JSON: &str = include_str!("../../../tools/ocr-sidecar/model-manifest.json");
const DOWNLOAD_EVENT: &str = "document-converter:model-progress";
const SAFETY_MARGIN_BYTES: u64 = 1024 * 1024 * 1024;
const DOWNLOAD_ATTEMPTS: u32 = 4;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelManifest {
    pub schema_version: u32,
    pub models: std::collections::BTreeMap<String, ModelDefinition>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDefinition {
    pub display_name: String,
    pub version: String,
    pub repository: String,
    pub revision: String,
    pub architecture: String,
    pub download_size_bytes: u64,
    pub installed_size_bytes: u64,
    pub license: String,
    pub minimum_sidecar_version: String,
    pub protocol_version: u32,
    pub files: Vec<ModelFile>,
    pub download: ModelDownload,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFile {
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownload {
    pub scheme: String,
    pub base_url: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstallationMarker {
    manifest_schema_version: u32,
    model_id: String,
    model_version: String,
    revision: String,
    verified_at: String,
}

#[derive(Clone)]
pub struct ModelManager {
    app: AppHandle,
    root: PathBuf,
    manifest: Arc<ModelManifest>,
    cancel: Arc<AtomicBool>,
    install_lock: Arc<Mutex<()>>,
    verification: Arc<StdMutex<Option<bool>>>,
}

impl ModelManager {
    pub fn new(app: AppHandle, root: PathBuf) -> ConverterResult<Self> {
        let manifest = parse_manifest()?;
        fs::create_dir_all(&root)?;
        Ok(Self {
            app,
            root,
            manifest: Arc::new(manifest),
            cancel: Arc::new(AtomicBool::new(false)),
            install_lock: Arc::new(Mutex::new(())),
            verification: Arc::new(StdMutex::new(None)),
        })
    }

    pub fn definition(&self) -> ConverterResult<&ModelDefinition> {
        self.manifest.models.get(OCR_ENGINE_ID).ok_or_else(|| {
            ConverterError::new(
                "MODEL_MANIFEST_INVALID",
                "Document AI model is not configured",
            )
        })
    }

    pub fn installed_path(&self) -> ConverterResult<PathBuf> {
        Ok(self
            .root
            .join(OCR_ENGINE_ID)
            .join(&self.definition()?.version))
    }

    pub fn status(&self) -> ConverterResult<ModelStatus> {
        let definition = self.definition()?;
        let install_path = self.installed_path()?;
        let marker_path = install_path.join(".agentic-os-model.json");
        let installed = marker_path.is_file();
        let mut checksum_valid = false;
        let mut error_code = None;
        let mut error_message = None;

        if installed {
            let cached = *self
                .verification
                .lock()
                .expect("model verification mutex poisoned");
            let verification = cached.unwrap_or_else(|| {
                let valid = self.verify_at(&install_path).is_ok();
                *self
                    .verification
                    .lock()
                    .expect("model verification mutex poisoned") = Some(valid);
                valid
            });
            checksum_valid = verification;
            if !verification {
                error_code = Some("MODEL_CHECKSUM_FAILED".to_string());
                error_message = Some("Document AI needs repair".to_string());
            }
        }

        Ok(ModelStatus {
            id: OCR_ENGINE_ID.to_string(),
            display_name: definition.display_name.clone(),
            version: definition.version.clone(),
            state: if checksum_valid {
                "ready"
            } else if installed {
                "repair-required"
            } else {
                "not-installed"
            }
            .to_string(),
            installed,
            checksum_valid,
            download_size_bytes: definition.download_size_bytes,
            installed_size_bytes: if installed {
                directory_size(&install_path).unwrap_or(0)
            } else {
                definition.installed_size_bytes
            },
            installed_path: installed.then(|| install_path.to_string_lossy().to_string()),
            architecture: definition.architecture.clone(),
            license: definition.license.clone(),
            error_code,
            error_message,
        })
    }

    pub async fn install(&self) -> ConverterResult<ModelStatus> {
        let _guard = self.install_lock.lock().await;
        self.cancel.store(false, Ordering::SeqCst);
        if self.status()?.checksum_valid {
            return self.status();
        }
        self.validate_runtime()?;
        let definition = self.definition()?.clone();
        let available = available_space(&self.root)?;
        let required = definition
            .download_size_bytes
            .saturating_add(definition.installed_size_bytes)
            .saturating_add(SAFETY_MARGIN_BYTES);
        if available < required {
            return Err(ConverterError::new(
                "INSUFFICIENT_DISK_SPACE",
                format!(
                    "Document AI needs about {:.1} GB of free space during installation",
                    required as f64 / 1_073_741_824.0
                ),
            ));
        }

        let downloads_root = self.root.join(".downloads");
        fs::create_dir_all(&downloads_root)?;
        let staging = downloads_root.join(Uuid::new_v4().to_string());
        fs::create_dir(&staging)?;
        let result = self.download_all(&definition, &staging).await;
        if let Err(error) = result {
            let _ = remove_managed_tree(&downloads_root, &staging);
            return Err(error);
        }
        if let Err(error) = self.verify_at(&staging) {
            let _ = remove_managed_tree(&downloads_root, &staging);
            return Err(error);
        }

        let marker = InstallationMarker {
            manifest_schema_version: self.manifest.schema_version,
            model_id: OCR_ENGINE_ID.to_string(),
            model_version: definition.version.clone(),
            revision: definition.revision.clone(),
            verified_at: chrono::Utc::now().to_rfc3339(),
        };
        if let Err(error) = write_atomic(
            &staging.join(".agentic-os-model.json"),
            &serde_json::to_vec_pretty(&marker)?,
        ) {
            let _ = remove_managed_tree(&downloads_root, &staging);
            return Err(error);
        }

        let final_path = self.installed_path()?;
        if let Some(parent) = final_path.parent() {
            fs::create_dir_all(parent)?;
        }
        if final_path.exists() {
            remove_managed_tree(&self.root, &final_path)?;
        }
        if let Err(error) = fs::rename(&staging, &final_path) {
            let _ = remove_managed_tree(&downloads_root, &staging);
            return Err(error.into());
        }
        *self
            .verification
            .lock()
            .expect("model verification mutex poisoned") = Some(true);
        let _ = fs::remove_dir(&downloads_root);
        self.emit_progress(
            "installed",
            definition.download_size_bytes,
            definition.download_size_bytes,
            None,
        );
        self.status()
    }

    pub fn cancel_install(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }

    pub async fn repair(&self) -> ConverterResult<ModelStatus> {
        self.remove()?;
        self.install().await
    }

    pub fn remove(&self) -> ConverterResult<ModelStatus> {
        self.cancel_install();
        let path = self.installed_path()?;
        if path.exists() {
            remove_managed_tree(&self.root, &path)?;
        }
        *self
            .verification
            .lock()
            .expect("model verification mutex poisoned") = None;
        self.status()
    }

    async fn download_all(
        &self,
        definition: &ModelDefinition,
        staging: &Path,
    ) -> ConverterResult<()> {
        let client = reqwest::Client::builder()
            .https_only(true)
            .user_agent(format!("Agentic-OS/{}", env!("CARGO_PKG_VERSION")))
            .build()?;
        let mut downloaded = 0u64;
        for file in &definition.files {
            self.ensure_not_cancelled()?;
            let relative = safe_model_relative(&file.path)?;
            let destination = staging.join(relative);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut last_error = None;
            for attempt in 1..=DOWNLOAD_ATTEMPTS {
                match self
                    .download_file(&client, definition, file, &destination, downloaded)
                    .await
                {
                    Ok(file_bytes) => {
                        downloaded += file_bytes;
                        last_error = None;
                        break;
                    }
                    Err(error)
                        if error.code() == "MODEL_DOWNLOAD_FAILED"
                            && attempt < DOWNLOAD_ATTEMPTS =>
                    {
                        log::warn!(
                            "Document AI file download attempt {attempt}/{DOWNLOAD_ATTEMPTS} failed for {}",
                            file.path
                        );
                        last_error = Some(error);
                        tokio::time::sleep(std::time::Duration::from_secs(1 << (attempt - 1)))
                            .await;
                    }
                    Err(error) => return Err(error),
                }
            }
            if let Some(error) = last_error {
                return Err(error);
            }
        }
        Ok(())
    }

    async fn download_file(
        &self,
        client: &reqwest::Client,
        definition: &ModelDefinition,
        file: &ModelFile,
        destination: &Path,
        downloaded: u64,
    ) -> ConverterResult<u64> {
        let temporary = destination.with_extension(format!("download-{}", Uuid::new_v4()));
        let result = async {
            let url = format!("{}{}", definition.download.base_url, file.path);
            let response = client
                .get(url)
                .send()
                .await
                .map_err(redacted_download_error)?
                .error_for_status()
                .map_err(redacted_download_error)?;
            if let Some(length) = response.content_length() {
                if length != file.size_bytes {
                    return Err(ConverterError::new(
                        "MODEL_DOWNLOAD_FAILED",
                        "Document AI download size did not match the signed manifest",
                    ));
                }
            }

            let mut output = File::create(&temporary)?;
            let mut hasher = Sha256::new();
            let mut file_bytes = 0u64;
            let mut stream = response.bytes_stream();
            while let Some(chunk) = stream.next().await {
                self.ensure_not_cancelled()?;
                let chunk = chunk.map_err(redacted_download_error)?;
                output.write_all(&chunk)?;
                hasher.update(&chunk);
                file_bytes += chunk.len() as u64;
                self.emit_progress(
                    "downloading",
                    downloaded + file_bytes,
                    definition.download_size_bytes,
                    Some(file.path.clone()),
                );
            }
            output.sync_all()?;
            if file_bytes != file.size_bytes || format!("{:x}", hasher.finalize()) != file.sha256 {
                return Err(ConverterError::new(
                    "MODEL_CHECKSUM_FAILED",
                    "Document AI failed integrity verification",
                ));
            }
            fs::rename(&temporary, destination)?;
            Ok(file_bytes)
        }
        .await;

        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn verify_at(&self, path: &Path) -> ConverterResult<()> {
        let definition = self.definition()?;
        for expected in &definition.files {
            let relative = safe_model_relative(&expected.path)?;
            let file_path = path.join(relative);
            let metadata = fs::symlink_metadata(&file_path).map_err(|_| {
                ConverterError::new(
                    "MODEL_NOT_INSTALLED",
                    "Document AI is missing required files",
                )
            })?;
            if !metadata.is_file()
                || metadata.file_type().is_symlink()
                || metadata.len() != expected.size_bytes
            {
                return Err(ConverterError::new(
                    "MODEL_CHECKSUM_FAILED",
                    "Document AI needs repair",
                ));
            }
            let mut file = File::open(file_path)?;
            let mut hasher = Sha256::new();
            let mut buffer = [0u8; 1024 * 1024];
            loop {
                let read = file.read(&mut buffer)?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
            }
            if format!("{:x}", hasher.finalize()) != expected.sha256 {
                return Err(ConverterError::new(
                    "MODEL_CHECKSUM_FAILED",
                    "Document AI needs repair",
                ));
            }
        }
        Ok(())
    }

    fn validate_runtime(&self) -> ConverterResult<()> {
        let definition = self.definition()?;
        if std::env::consts::ARCH != "aarch64" || std::env::consts::OS != "macos" {
            return Err(ConverterError::new(
                "UNSUPPORTED_ARCHITECTURE",
                "Document AI v1 requires an Apple Silicon Mac",
            ));
        }
        if definition.download.scheme != "https"
            || !definition.download.base_url.starts_with("https://")
        {
            return Err(ConverterError::new(
                "MODEL_MANIFEST_INVALID",
                "Document AI download endpoint is not secure",
            ));
        }
        if definition.minimum_sidecar_version != OCR_SIDECAR_VERSION {
            return Err(ConverterError::new(
                "MODEL_INCOMPATIBLE",
                "Document AI components must be updated together",
            ));
        }
        if definition.protocol_version != SIDECAR_PROTOCOL_VERSION
            || definition.repository.is_empty()
        {
            return Err(ConverterError::new(
                "MODEL_INCOMPATIBLE",
                "Document AI manifest is incompatible with this app",
            ));
        }
        Ok(())
    }

    fn ensure_not_cancelled(&self) -> ConverterResult<()> {
        if self.cancel.load(Ordering::SeqCst) {
            Err(ConverterError::new(
                "MODEL_DOWNLOAD_CANCELLED",
                "Document AI installation was cancelled",
            ))
        } else {
            Ok(())
        }
    }

    fn emit_progress(
        &self,
        stage: &str,
        downloaded: u64,
        total: u64,
        current_file: Option<String>,
    ) {
        let _ = self.app.emit(
            DOWNLOAD_EVENT,
            ModelDownloadProgress {
                model_id: OCR_ENGINE_ID.to_string(),
                stage: stage.to_string(),
                downloaded_bytes: downloaded.min(total),
                total_bytes: total,
                percent: if total == 0 {
                    0.0
                } else {
                    (downloaded.min(total) as f64 / total as f64) * 100.0
                },
                current_file,
            },
        );
    }
}

fn redacted_download_error(error: reqwest::Error) -> ConverterError {
    log::warn!("Document AI download request failed: {error}");
    ConverterError::new(
        "MODEL_DOWNLOAD_FAILED",
        "Could not download Document AI. Check your connection and try again",
    )
}

fn parse_manifest() -> ConverterResult<ModelManifest> {
    let manifest: ModelManifest = serde_json::from_str(MODEL_MANIFEST_JSON).map_err(|_| {
        ConverterError::new("MODEL_MANIFEST_INVALID", "Document AI manifest is invalid")
    })?;
    if manifest.schema_version != 1 {
        return Err(ConverterError::new(
            "MODEL_MANIFEST_INVALID",
            "Document AI manifest version is not supported",
        ));
    }
    let model = manifest.models.get(OCR_ENGINE_ID).ok_or_else(|| {
        ConverterError::new("MODEL_MANIFEST_INVALID", "Document AI model is missing")
    })?;
    let sum = model.files.iter().map(|file| file.size_bytes).sum::<u64>();
    if sum != model.download_size_bytes || model.files.is_empty() {
        return Err(ConverterError::new(
            "MODEL_MANIFEST_INVALID",
            "Document AI manifest size is inconsistent",
        ));
    }
    for file in &model.files {
        safe_model_relative(&file.path)?;
        if file.sha256.len() != 64 || !file.sha256.chars().all(|value| value.is_ascii_hexdigit()) {
            return Err(ConverterError::new(
                "MODEL_MANIFEST_INVALID",
                "Document AI manifest checksum is invalid",
            ));
        }
    }
    Ok(manifest)
}

fn safe_model_relative(value: &str) -> ConverterResult<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, std::path::Component::Normal(_)))
    {
        return Err(ConverterError::new(
            "MODEL_MANIFEST_INVALID",
            "Document AI manifest contains an unsafe path",
        ));
    }
    Ok(path.to_path_buf())
}

fn available_space(path: &Path) -> ConverterResult<u64> {
    let c_path = CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        ConverterError::new(
            "INSUFFICIENT_DISK_SPACE",
            "Could not inspect available storage",
        )
    })?;
    let mut stats = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    let result = unsafe { libc::statvfs(c_path.as_ptr(), stats.as_mut_ptr()) };
    if result != 0 {
        return Err(ConverterError::new(
            "INSUFFICIENT_DISK_SPACE",
            "Could not inspect available storage",
        ));
    }
    let stats = unsafe { stats.assume_init() };
    Ok((stats.f_bavail as u64).saturating_mul(stats.f_frsize as u64))
}

fn write_atomic(path: &Path, bytes: &[u8]) -> ConverterResult<()> {
    let temporary = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    let mut file = File::create(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn remove_managed_tree(root: &Path, target: &Path) -> ConverterResult<()> {
    let root = root.canonicalize()?;
    let target_parent = target
        .parent()
        .ok_or_else(|| ConverterError::new("INVALID_PATH", "Managed model path has no parent"))?;
    let parent = target_parent.canonicalize()?;
    if !parent.starts_with(&root) {
        return Err(ConverterError::new(
            "INVALID_PATH",
            "Refusing to remove data outside the model directory",
        ));
    }
    let metadata = fs::symlink_metadata(target)?;
    if metadata.file_type().is_symlink() {
        return Err(ConverterError::new(
            "INVALID_PATH",
            "Refusing to follow a model directory symlink",
        ));
    }
    fs::remove_dir_all(target)?;
    Ok(())
}

fn directory_size(path: &Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(path).follow_links(false) {
        let entry = entry?;
        if entry.file_type().is_file() {
            total = total.saturating_add(entry.metadata()?.len());
        }
    }
    Ok(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_pinned_manifest() {
        let manifest = parse_manifest().unwrap();
        let model = manifest.models.get(OCR_ENGINE_ID).unwrap();
        assert_eq!(model.version, "1.6");
        assert_eq!(model.protocol_version, 1);
        assert_eq!(model.repository, "PaddlePaddle/PaddleOCR-VL-1.6");
    }

    #[test]
    fn rejects_path_traversal() {
        assert!(safe_model_relative("../model.safetensors").is_err());
        assert!(safe_model_relative("model.safetensors").is_ok());
    }
}
