use std::process::Stdio;

use serde_json::Value;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;

use crate::error::{AppError, AppResult};

const MAX_PROMPT_CHARS: usize = 64_000;
const MAX_STDOUT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_STDERR_BYTES: u64 = 16 * 1024;
const MODEL_TIMEOUT_SECS: u64 = 75;

#[derive(Debug, Clone)]
pub struct StructuredModelOutput {
    pub text: String,
    pub tokens: Option<i64>,
}

/// Executes one bounded, read-only Codex turn for deterministic application
/// features that need model synthesis but no tools. The prompt is provided on
/// stdin so questions and evidence do not appear in the process arguments.
pub async fn run_read_only_json(prompt: &str) -> AppResult<StructuredModelOutput> {
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(io_error(
            "structured model prompt exceeds 64,000 characters",
        ));
    }

    let work_dir = std::env::temp_dir().join("agentic-os-structured-model");
    std::fs::create_dir_all(&work_dir)?;

    let binary = super::resolve_binary("codex");
    let prompt = prompt.as_bytes().to_vec();
    let run = async move {
        let mut command = Command::new(&binary);
        command
            .arg("exec")
            .arg("--json")
            .arg("--skip-git-repo-check")
            .arg("-s")
            .arg("read-only")
            .arg("-C")
            .arg(&work_dir)
            .current_dir(&work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // GUI-launched macOS apps do not inherit the Homebrew PATH. The npm
        // Codex launcher uses `/usr/bin/env node`, so both the launcher and its
        // runtime must be discoverable even when Agentic OS was opened from Finder.
        let inherited_path = std::env::var("PATH").unwrap_or_default();
        command.env(
            "PATH",
            format!("/opt/homebrew/bin:/usr/local/bin:{inherited_path}"),
        );

        if let Some(vf_key) = super::resolve_vf_api_key() {
            command.env("VF_API_KEY", vf_key);
        }

        let mut child = command.spawn().map_err(|error| {
            io_error(format!(
                "AI synthesis is unavailable because '{binary} exec' could not start: {error}"
            ))
        })?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io_error("AI synthesis stdin is unavailable"))?;
        stdin.write_all(&prompt).await?;
        stdin.shutdown().await?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io_error("AI synthesis stdout is unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io_error("AI synthesis stderr is unavailable"))?;

        let read_stdout = async move {
            let mut bytes = Vec::new();
            stdout
                .take(MAX_STDOUT_BYTES + 1)
                .read_to_end(&mut bytes)
                .await?;
            Ok::<_, std::io::Error>(bytes)
        };
        let read_stderr = async move {
            let mut bytes = Vec::new();
            stderr
                .take(MAX_STDERR_BYTES + 1)
                .read_to_end(&mut bytes)
                .await?;
            Ok::<_, std::io::Error>(bytes)
        };
        let (stdout, stderr, status) = tokio::try_join!(read_stdout, read_stderr, child.wait())?;

        if stdout.len() as u64 > MAX_STDOUT_BYTES {
            return Err(io_error(
                "AI synthesis output exceeded the 2 MiB safety limit",
            ));
        }
        if stderr.len() as u64 > MAX_STDERR_BYTES {
            return Err(io_error(
                "AI synthesis error output exceeded the safety limit",
            ));
        }
        if !status.success() {
            let detail = String::from_utf8_lossy(&stderr).trim().to_string();
            if detail.contains("ENOENT") && detail.contains("codex") {
                return Err(io_error(
                    "AI synthesis is unavailable because the Codex CLI installation is incomplete. Reinstall the Codex CLI, then retry Ask.",
                ));
            }
            return Err(io_error(if detail.is_empty() {
                format!("AI synthesis exited with status {status}")
            } else {
                format!("AI synthesis failed: {detail}")
            }));
        }

        parse_jsonl_output(&stdout)
    };

    tokio::time::timeout(std::time::Duration::from_secs(MODEL_TIMEOUT_SECS), run)
        .await
        .map_err(|_| {
            io_error(format!(
                "AI synthesis exceeded the {MODEL_TIMEOUT_SECS} second limit"
            ))
        })?
}

fn parse_jsonl_output(stdout: &[u8]) -> AppResult<StructuredModelOutput> {
    let text = String::from_utf8(stdout.to_vec())
        .map_err(|_| io_error("AI synthesis returned non-UTF-8 output"))?;
    let mut final_message = None;
    let mut tokens = None;
    let mut failure = None;

    for line in text.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        match value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "item.completed" => {
                let item = value.get("item").unwrap_or(&Value::Null);
                if item.get("type").and_then(Value::as_str) == Some("agent_message") {
                    if let Some(message) = item.get("text").and_then(Value::as_str) {
                        final_message = Some(message.to_string());
                    }
                }
            }
            "turn.completed" => {
                let usage = value.get("usage").unwrap_or(&Value::Null);
                let input = usage
                    .get("input_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or_default();
                let output = usage
                    .get("output_tokens")
                    .and_then(Value::as_i64)
                    .unwrap_or_default();
                if input + output > 0 {
                    tokens = Some(input + output);
                }
            }
            "turn.failed" | "error" => {
                failure = value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .or_else(|| value.get("message").and_then(Value::as_str))
                    .map(str::to_string);
            }
            _ => {}
        }
    }

    if let Some(failure) = failure {
        return Err(io_error(format!("AI synthesis failed: {failure}")));
    }
    let text = final_message
        .filter(|message| !message.trim().is_empty())
        .ok_or_else(|| io_error("AI synthesis returned no final answer"))?;
    Ok(StructuredModelOutput { text, tokens })
}

fn io_error(message: impl Into<String>) -> AppError {
    AppError::Io(std::io::Error::other(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_last_agent_message_and_usage() {
        let output = br#"{"type":"item.completed","item":{"type":"agent_message","text":"{\"abstained\":false}"}}
{"type":"turn.completed","usage":{"input_tokens":30,"output_tokens":12}}
"#;
        let parsed = parse_jsonl_output(output).unwrap();
        assert_eq!(parsed.text, r#"{"abstained":false}"#);
        assert_eq!(parsed.tokens, Some(42));
    }
}
