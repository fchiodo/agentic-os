use serde::{Deserialize, Serialize};

use super::errors::{ConverterError, ConverterResult};
use super::types::{EngineConversion, RawPage, SIDECAR_PROTOCOL_VERSION};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarRequest<'a> {
    pub protocol_version: u32,
    pub command: &'a str,
    pub request_id: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_path: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processing_mode: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

impl<'a> SidecarRequest<'a> {
    pub fn simple(command: &'a str, request_id: &'a str) -> Self {
        Self {
            protocol_version: SIDECAR_PROTOCOL_VERSION,
            command,
            request_id,
            job_id: None,
            input_path: None,
            working_directory: None,
            model_path: None,
            processing_mode: None,
            max_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarHealth {
    pub sidecar_version: String,
    pub engine: String,
    pub engine_version: String,
    pub model_required: String,
    pub architecture: String,
    pub process_id: u32,
    pub process_group_id: i32,
}

#[derive(Debug, Clone)]
pub enum SidecarMessage {
    Health {
        request_id: String,
        health: SidecarHealth,
    },
    Progress {
        request_id: String,
        job_id: Option<String>,
        stage: String,
        page: Option<u32>,
        total_pages: Option<u32>,
        label: String,
    },
    Completed {
        request_id: String,
        job_id: Option<String>,
        result: EngineConversion,
    },
    Acknowledged {
        _request_id: String,
        _message_type: String,
    },
    Error {
        request_id: Option<String>,
        code: String,
        message: String,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    protocol_version: u32,
    #[serde(rename = "type")]
    message_type: String,
    request_id: Option<String>,
    job_id: Option<String>,
    sidecar_version: Option<String>,
    engine: Option<String>,
    engine_version: Option<String>,
    model_required: Option<String>,
    architecture: Option<String>,
    process_id: Option<u32>,
    process_group_id: Option<i32>,
    stage: Option<String>,
    page: Option<u32>,
    total_pages: Option<u32>,
    label: Option<String>,
    code: Option<String>,
    message: Option<String>,
    result: Option<SidecarConversion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SidecarConversion {
    pages: Vec<SidecarPage>,
    total_pages: u32,
    pages_digital: u32,
    pages_ocr: u32,
    processing_mode: String,
    #[serde(default)]
    warnings: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SidecarPage {
    page_number: u32,
    source_mode: String,
    text: String,
    confidence: Option<f64>,
    #[serde(default)]
    rotation_correction: i32,
    rendered_asset_path: Option<String>,
}

impl From<SidecarConversion> for EngineConversion {
    fn from(value: SidecarConversion) -> Self {
        Self {
            pages: value
                .pages
                .into_iter()
                .map(|page| RawPage {
                    page_number: page.page_number,
                    source_mode: page.source_mode,
                    text: page.text,
                    confidence: page.confidence,
                    rotation_correction: page.rotation_correction,
                    rendered_asset_path: page.rendered_asset_path,
                })
                .collect(),
            total_pages: value.total_pages,
            pages_digital: value.pages_digital,
            pages_ocr: value.pages_ocr,
            processing_mode: value.processing_mode,
            warnings: value.warnings,
        }
    }
}

pub fn encode_request(request: &SidecarRequest<'_>) -> ConverterResult<Vec<u8>> {
    let mut value = serde_json::to_vec(request)?;
    value.push(b'\n');
    Ok(value)
}

pub fn parse_message(line: &[u8]) -> ConverterResult<SidecarMessage> {
    if line.len() > 8 * 1024 * 1024 {
        return Err(ConverterError::new(
            "SIDECAR_PROTOCOL_ERROR",
            "Document AI returned an oversized response",
        ));
    }
    let envelope: Envelope = serde_json::from_slice(line).map_err(|_| {
        ConverterError::new(
            "SIDECAR_PROTOCOL_ERROR",
            "Document AI returned an invalid response",
        )
    })?;
    if envelope.protocol_version != SIDECAR_PROTOCOL_VERSION {
        return Err(ConverterError::new(
            "PROTOCOL_MISMATCH",
            "Document AI must be repaired or updated",
        ));
    }
    let required_request_id = || {
        envelope.request_id.clone().ok_or_else(|| {
            ConverterError::new("SIDECAR_PROTOCOL_ERROR", "Response is missing requestId")
        })
    };
    match envelope.message_type.as_str() {
        "health" => Ok(SidecarMessage::Health {
            request_id: required_request_id()?,
            health: SidecarHealth {
                sidecar_version: required(envelope.sidecar_version, "sidecarVersion")?,
                engine: required(envelope.engine, "engine")?,
                engine_version: required(envelope.engine_version, "engineVersion")?,
                model_required: required(envelope.model_required, "modelRequired")?,
                architecture: required(envelope.architecture, "architecture")?,
                process_id: required(envelope.process_id, "processId")?,
                process_group_id: required(envelope.process_group_id, "processGroupId")?,
            },
        }),
        "progress" => Ok(SidecarMessage::Progress {
            request_id: required_request_id()?,
            job_id: envelope.job_id,
            stage: required(envelope.stage, "stage")?,
            page: envelope.page,
            total_pages: envelope.total_pages,
            label: envelope
                .label
                .unwrap_or_else(|| "Processing document".to_string()),
        }),
        "completed" => Ok(SidecarMessage::Completed {
            request_id: required_request_id()?,
            job_id: envelope.job_id,
            result: required(envelope.result, "result")?.into(),
        }),
        "model-loaded" | "capabilities" | "shutdown" => Ok(SidecarMessage::Acknowledged {
            _request_id: required_request_id()?,
            _message_type: envelope.message_type,
        }),
        "error" => Ok(SidecarMessage::Error {
            request_id: envelope.request_id,
            code: envelope
                .code
                .unwrap_or_else(|| "OCR_ENGINE_FAILED".to_string()),
            message: envelope
                .message
                .unwrap_or_else(|| "Document AI could not process this file".to_string()),
        }),
        _ => Err(ConverterError::new(
            "SIDECAR_PROTOCOL_ERROR",
            "Document AI returned an unsupported response",
        )),
    }
}

fn required<T>(value: Option<T>, field: &str) -> ConverterResult<T> {
    value.ok_or_else(|| {
        ConverterError::new(
            "SIDECAR_PROTOCOL_ERROR",
            format!("Response is missing {field}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_malformed_messages() {
        assert_eq!(
            parse_message(b"not-json").unwrap_err().code(),
            "SIDECAR_PROTOCOL_ERROR"
        );
    }

    #[test]
    fn rejects_incompatible_protocol() {
        let message = br#"{"protocolVersion":2,"type":"health","requestId":"one"}"#;
        assert_eq!(
            parse_message(message).unwrap_err().code(),
            "PROTOCOL_MISMATCH"
        );
    }

    #[test]
    fn parses_progress_without_human_string_matching() {
        let message = br#"{"protocolVersion":1,"type":"progress","requestId":"one","jobId":"job","stage":"ocr","page":2,"totalPages":4,"label":"Recognizing"}"#;
        match parse_message(message).unwrap() {
            SidecarMessage::Progress { stage, page, .. } => {
                assert_eq!(stage, "ocr");
                assert_eq!(page, Some(2));
            }
            _ => panic!("unexpected message"),
        }
    }

    #[test]
    fn parses_private_sidecar_process_group() {
        let message = br#"{"protocolVersion":1,"type":"health","requestId":"one","sidecarVersion":"0.2.1","engine":"paddleocr-vl","engineVersion":"0.7.2","modelRequired":"pinned","architecture":"arm64","processId":1234,"processGroupId":1234}"#;
        match parse_message(message).unwrap() {
            SidecarMessage::Health { health, .. } => {
                assert_eq!(health.process_id, 1234);
                assert_eq!(health.process_group_id, 1234);
            }
            _ => panic!("unexpected message"),
        }
    }
}
