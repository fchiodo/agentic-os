use std::fmt;

use thiserror::Error;

pub type ConverterResult<T> = Result<T, ConverterError>;

#[derive(Debug, Error)]
pub enum ConverterError {
    #[error("{code}|{message}")]
    Structured { code: &'static str, message: String },
    #[error("{0}")]
    Internal(String),
}

impl ConverterError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self::Structured {
            code,
            message: message.into(),
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::Structured { code, .. } => code,
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }

    pub fn public_message(&self) -> String {
        match self {
            Self::Structured { message, .. } => message.clone(),
            Self::Internal(_) => "Document Converter encountered an internal error".to_string(),
        }
    }
}

impl From<std::io::Error> for ConverterError {
    fn from(error: std::io::Error) -> Self {
        let code = if error.raw_os_error() == Some(libc::ENOSPC) {
            "INSUFFICIENT_DISK_SPACE"
        } else {
            "OUTPUT_WRITE_FAILED"
        };
        Self::new(code, error.to_string())
    }
}

impl From<rusqlite::Error> for ConverterError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Internal(format!("database error: {error}"))
    }
}

impl From<crate::error::AppError> for ConverterError {
    fn from(error: crate::error::AppError) -> Self {
        Self::Internal(format!("application error: {error}"))
    }
}

impl From<serde_json::Error> for ConverterError {
    fn from(error: serde_json::Error) -> Self {
        Self::Internal(format!("JSON error: {error}"))
    }
}

impl From<tauri_plugin_shell::Error> for ConverterError {
    fn from(error: tauri_plugin_shell::Error) -> Self {
        Self::new("SIDECAR_CRASHED", error.to_string())
    }
}

impl From<reqwest::Error> for ConverterError {
    fn from(error: reqwest::Error) -> Self {
        log::warn!("Document AI network operation failed: {error}");
        Self::new(
            "MODEL_DOWNLOAD_FAILED",
            "Could not download Document AI. Check your connection and try again",
        )
    }
}

impl From<fmt::Error> for ConverterError {
    fn from(error: fmt::Error) -> Self {
        Self::Internal(error.to_string())
    }
}
