use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use serde::Deserialize;
use tauri::AppHandle;
use tauri_plugin_shell::{process::CommandEvent, ShellExt};

const MARKITDOWN_TIMEOUT_SECS: u64 = 25;
const MAX_SIDECAR_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const PDF_EXTRACT_VERSION: &str = "0.8.2";

#[derive(Debug, Clone)]
pub struct ExtractionQuality {
    pub score: i64,
    pub status: &'static str,
    pub issues: Vec<String>,
}

impl ExtractionQuality {
    fn passed(&self) -> bool {
        self.status == "passed"
    }
}

#[derive(Debug)]
pub struct PdfExtractionOutcome {
    /// Searchable source snapshot. It contains extracted text only after the
    /// quality gate passes; rejected output is replaced with a diagnostic.
    pub snapshot_body: String,
    /// Input for fact proposal generation. Empty when quality is insufficient.
    pub extraction_text: String,
    /// Best-effort text used only by deterministic secret/injection scanners.
    pub security_scan_text: String,
    pub engine: Option<String>,
    pub version: Option<String>,
    pub quality: ExtractionQuality,
    pub warnings: Vec<String>,
}

#[derive(Debug)]
struct ExtractionAttempt {
    engine: String,
    version: String,
    text: String,
    quality: ExtractionQuality,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MarkItDownResponse {
    engine: String,
    version: String,
    markdown: String,
}

/// Run the complete local extraction cascade. MarkItDown is the preferred
/// converter in the desktop runtime; the in-process Rust parser is an
/// independent fallback. No output can reach search or proposal generation
/// unless it passes the deterministic quality gate.
pub async fn extract_pdf(app: Option<&AppHandle>, bytes: &[u8]) -> PdfExtractionOutcome {
    let mut warnings = Vec::new();
    let mut rejected_attempts = Vec::new();

    if let Some(app) = app {
        match run_markitdown(app, bytes).await {
            Ok((text, version)) => {
                let normalized = normalize_pdf_text(&text);
                let attempt = ExtractionAttempt {
                    engine: "markitdown".to_string(),
                    version,
                    quality: assess_text_quality(&normalized),
                    text: normalized,
                };
                if attempt.quality.passed() {
                    return accepted_outcome(attempt, warnings);
                }
                warnings.push(format!(
                    "MarkItDown output did not pass the PDF quality gate (score {}/100); the local fallback was evaluated.",
                    attempt.quality.score
                ));
                rejected_attempts.push(attempt);
            }
            Err(error) => warnings.push(format!(
                "MarkItDown could not complete the conversion ({error}); the local fallback was evaluated."
            )),
        }
    } else {
        warnings.push(
            "MarkItDown sidecar was unavailable in this execution context; the local fallback was evaluated."
                .to_string(),
        );
    }

    match run_pdf_extract(bytes).await {
        Ok(text) => {
            let normalized = normalize_pdf_text(&text);
            let attempt = ExtractionAttempt {
                engine: "pdf-extract".to_string(),
                version: PDF_EXTRACT_VERSION.to_string(),
                quality: assess_text_quality(&normalized),
                text: normalized,
            };
            if attempt.quality.passed() {
                warnings.push(
                    "The bundled local Rust fallback produced the accepted PDF text.".to_string(),
                );
                return accepted_outcome(attempt, warnings);
            }
            warnings.push(format!(
                "The local fallback did not pass the PDF quality gate (score {}/100).",
                attempt.quality.score
            ));
            rejected_attempts.push(attempt);
        }
        Err(error) => warnings.push(format!("The local PDF fallback failed ({error}).")),
    }

    rejected_outcome(rejected_attempts, warnings)
}

fn accepted_outcome(attempt: ExtractionAttempt, mut warnings: Vec<String>) -> PdfExtractionOutcome {
    warnings.push(format!(
        "PDF text was extracted locally with {} {} and passed the quality gate ({}/100). The original PDF was preserved byte-for-byte beside the source snapshot.",
        attempt.engine, attempt.version, attempt.quality.score
    ));
    PdfExtractionOutcome {
        snapshot_body: attempt.text.clone(),
        extraction_text: attempt.text.clone(),
        security_scan_text: attempt.text,
        engine: Some(attempt.engine),
        version: Some(attempt.version),
        quality: attempt.quality,
        warnings,
    }
}

fn rejected_outcome(
    attempts: Vec<ExtractionAttempt>,
    mut warnings: Vec<String>,
) -> PdfExtractionOutcome {
    let best = attempts
        .into_iter()
        .max_by_key(|attempt| attempt.quality.score);
    let (engine, version, quality, security_scan_text) = if let Some(attempt) = best {
        (
            Some(attempt.engine),
            Some(attempt.version),
            attempt.quality,
            attempt.text,
        )
    } else {
        (
            None,
            None,
            ExtractionQuality {
                score: 0,
                status: "failed",
                issues: vec!["No machine-readable text was produced.".to_string()],
            },
            String::new(),
        )
    };
    warnings.push(
        "PDF extraction was blocked by the quality gate. The original remains preserved, corrupted text was not indexed, and no memory facts were proposed."
            .to_string(),
    );
    let issue_lines = quality
        .issues
        .iter()
        .map(|issue| format!("- {issue}"))
        .collect::<Vec<_>>()
        .join("\n");
    let snapshot_body = format!(
        "# PDF extraction blocked\n\nThe original PDF was preserved byte-for-byte. Extracted text was not admitted to search or memory proposals because it failed the deterministic quality gate.\n\nQuality score: {}/100\n\n{}",
        quality.score,
        if issue_lines.is_empty() {
            "- The converter did not return usable text.".to_string()
        } else {
            issue_lines
        }
    );
    PdfExtractionOutcome {
        snapshot_body,
        extraction_text: String::new(),
        security_scan_text,
        engine,
        version,
        quality,
        warnings,
    }
}

async fn run_markitdown(app: &AppHandle, bytes: &[u8]) -> Result<(String, String), String> {
    let request = serde_json::to_vec(&serde_json::json!({
        "pdfBase64": BASE64_STANDARD.encode(bytes),
    }))
    .map_err(|error| format!("request serialization failed: {error}"))?;
    let mut stdin = request;
    stdin.push(b'\n');

    let command = app
        .shell()
        .sidecar("markitdown-sidecar")
        .map_err(|error| format!("sidecar is unavailable: {error}"))?
        .env_clear()
        .env("LANG", "C.UTF-8")
        .set_raw_out(true);
    let (mut events, mut child) = command
        .spawn()
        .map_err(|error| format!("sidecar could not start: {error}"))?;
    if let Err(error) = child.write(&stdin) {
        let _ = child.kill();
        return Err(format!("sidecar input failed: {error}"));
    }

    let collect = async {
        let mut stdout = Vec::new();
        let mut stderr = Vec::new();
        let mut exit_code = None;
        while let Some(event) = events.recv().await {
            match event {
                CommandEvent::Stdout(chunk) => {
                    if stdout.len().saturating_add(chunk.len()) > MAX_SIDECAR_OUTPUT_BYTES {
                        return Err("sidecar output exceeded the 8 MiB safety limit".to_string());
                    }
                    stdout.extend_from_slice(&chunk);
                }
                CommandEvent::Stderr(chunk) => {
                    let remaining = 4_096usize.saturating_sub(stderr.len());
                    stderr.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
                }
                CommandEvent::Error(error) => return Err(format!("sidecar error: {error}")),
                CommandEvent::Terminated(payload) => {
                    exit_code = payload.code;
                    break;
                }
                _ => {}
            }
        }
        if exit_code != Some(0) {
            let detail = String::from_utf8_lossy(&stderr).trim().to_string();
            return Err(if detail.is_empty() {
                format!("sidecar exited with status {exit_code:?}")
            } else {
                format!("sidecar exited with status {exit_code:?}: {detail}")
            });
        }
        let response: MarkItDownResponse = serde_json::from_slice(&stdout)
            .map_err(|_| "sidecar returned malformed JSON".to_string())?;
        if response.engine != "markitdown" || response.version.trim().is_empty() {
            return Err("sidecar returned invalid converter metadata".to_string());
        }
        Ok((response.markdown, response.version))
    };

    match tokio::time::timeout(
        std::time::Duration::from_secs(MARKITDOWN_TIMEOUT_SECS),
        collect,
    )
    .await
    {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(error)) => {
            let _ = child.kill();
            Err(error)
        }
        Err(_) => {
            let _ = child.kill();
            Err(format!(
                "conversion exceeded the {MARKITDOWN_TIMEOUT_SECS} second safety limit"
            ))
        }
    }
}

async fn run_pdf_extract(bytes: &[u8]) -> Result<String, String> {
    let extraction_bytes = bytes.to_vec();
    tokio::time::timeout(
        std::time::Duration::from_secs(20),
        tokio::task::spawn_blocking(move || {
            std::panic::catch_unwind(|| pdf_extract::extract_text_from_mem(&extraction_bytes))
        }),
    )
    .await
    .map_err(|_| "conversion exceeded the 20 second safety limit".to_string())?
    .map_err(|_| "extraction worker failed".to_string())?
    .map_err(|_| "parser stopped while reading an invalid document".to_string())?
    .map_err(|error| format!("document may be encrypted or malformed: {error}"))
}

pub fn normalize_pdf_text(value: &str) -> String {
    value
        .replace('\0', "")
        .replace('\u{000c}', "\n\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn assess_text_quality(value: &str) -> ExtractionQuality {
    let visible_count = value
        .chars()
        .filter(|character| !character.is_whitespace())
        .count();
    let alphanumeric_count = value
        .chars()
        .filter(|character| character.is_alphanumeric())
        .count();
    let replacement_count = value
        .chars()
        .filter(|character| *character == '\u{fffd}')
        .count();
    let control_count = value
        .chars()
        .filter(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
        .count();
    let tokens = value
        .split_whitespace()
        .filter_map(|token| {
            let core = token.trim_matches(|character: char| !character.is_alphanumeric());
            (!core.is_empty()).then_some(core)
        })
        .collect::<Vec<_>>();
    let single_character_tokens = tokens
        .iter()
        .filter(|token| token.chars().count() == 1)
        .count();
    let single_ratio = if tokens.is_empty() {
        1.0
    } else {
        single_character_tokens as f64 / tokens.len() as f64
    };
    let mut longest_single_run = 0usize;
    let mut current_run = 0usize;
    for token in &tokens {
        if token.chars().count() == 1 {
            current_run += 1;
            longest_single_run = longest_single_run.max(current_run);
        } else {
            current_run = 0;
        }
    }
    let alphanumeric_ratio = if visible_count == 0 {
        0.0
    } else {
        alphanumeric_count as f64 / visible_count as f64
    };

    let mut score = 100i64;
    let mut issues = Vec::new();
    if visible_count < 40 || tokens.len() < 8 {
        score -= 65;
        issues.push("Too little machine-readable text was extracted.".to_string());
    }
    if longest_single_run >= 8 {
        score -= 70;
        issues.push(format!(
            "Detected a run of {longest_single_run} isolated glyphs, typical of letter-by-letter PDF corruption."
        ));
    } else if longest_single_run >= 5 {
        score -= 30;
        issues.push(format!(
            "Detected a suspicious run of {longest_single_run} isolated characters."
        ));
    }
    if tokens.len() >= 12 && single_ratio > 0.22 {
        score -= 55;
        issues.push(format!(
            "{:.0}% of extracted tokens contain only one character.",
            single_ratio * 100.0
        ));
    } else if tokens.len() >= 12 && single_ratio > 0.12 {
        score -= 20;
        issues.push(format!(
            "{:.0}% of extracted tokens contain only one character.",
            single_ratio * 100.0
        ));
    }
    if alphanumeric_ratio < 0.45 {
        score -= 35;
        issues
            .push("The extracted text contains too little readable language content.".to_string());
    }
    if replacement_count > 0 || control_count > 0 {
        score -= 30;
        issues.push(format!(
            "The output contains {replacement_count} replacement glyph(s) and {control_count} unsupported control character(s)."
        ));
    }
    score = score.clamp(0, 100);
    let has_fatal_spacing = longest_single_run >= 8 || (tokens.len() >= 12 && single_ratio > 0.22);
    let status = if score >= 70 && !has_fatal_spacing && visible_count >= 40 {
        "passed"
    } else {
        "failed"
    };
    ExtractionQuality {
        score,
        status,
        issues,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quality_gate_accepts_normal_document_text() {
        let quality = assess_text_quality(
            "Headless API authentication requires OAuth client credentials with short-lived JWT tokens. The compatibility date is 2025-02-01.",
        );
        assert_eq!(quality.status, "passed");
        assert!(quality.score >= 70);
    }

    #[test]
    fn quality_gate_rejects_letter_by_letter_pdf_output() {
        let quality = assess_text_quality(
            "E x a m p l e 1 d a y w i n d o w J a n u a r y 1 2 0 2 6 U T C t h r o u g h J a n u a r y 2 2 0 2 6 U T C",
        );
        assert_eq!(quality.status, "failed");
        assert!(quality
            .issues
            .iter()
            .any(|issue| issue.contains("isolated glyphs")));
    }

    #[test]
    fn rejected_output_never_becomes_candidate_text() {
        let attempt = ExtractionAttempt {
            engine: "markitdown".to_string(),
            version: "test".to_string(),
            text: "E x a m p l e t e x t i s b r o k e n".to_string(),
            quality: assess_text_quality("E x a m p l e t e x t i s b r o k e n"),
        };
        let outcome = rejected_outcome(vec![attempt], Vec::new());
        assert!(outcome.extraction_text.is_empty());
        assert!(outcome.snapshot_body.contains("extraction blocked"));
        assert_eq!(outcome.quality.status, "failed");
    }
}
