use std::process::Stdio;

use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;

use crate::error::{AppError, AppResult};

const MAX_PROMPT_CHARS: usize = 64_000;
const MAX_STDOUT_BYTES: u64 = 2 * 1024 * 1024;
const MAX_STDERR_BYTES: u64 = 16 * 1024;

/// The exact error message surfaced when the user stops a run. The frontend
/// matches on this string to render a quiet "stopped" state instead of an
/// error alert — keep the two in sync.
pub const STOPPED_BY_USER: &str = "Ask stopped by user";

/// Clonable cancellation signal. One user action (an Ask) may run several
/// model turns — query planning, then synthesis — and a single Stop click
/// must reach whichever turn is active, so the signal is a watch channel
/// rather than a one-shot.
pub type CancelSignal = tokio::sync::watch::Receiver<bool>;

/// A signal that can never fire (the sender is dropped immediately); used
/// by callers with no user-facing stop control (lint, import enrichment).
pub fn no_cancel() -> CancelSignal {
    tokio::sync::watch::channel(false).1
}

/// How long codex may stay completely silent before a Waiting marker is
/// emitted so the user sees the run is stalled rather than progressing.
const WAIT_NOTICE_SECS: u64 = 15;
const MAX_DIAGNOSTIC_CHARS: usize = 200;

#[derive(Debug, Clone)]
pub struct StructuredModelOutput {
    pub text: String,
    pub tokens: Option<i64>,
}

/// Structural progress markers surfaced while the model turn is running.
/// Deliberately metadata-only: model text never travels on this channel, so
/// nothing can reach the UI before the citation verifier has approved it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SynthesisProgress {
    ProcessSpawned,
    SessionStarted,
    TurnStarted,
    Reasoning,
    AnswerDrafted,
    TokensUsed { tokens: i64 },
    /// A codex stderr line, surfaced so a stalled run explains itself.
    Diagnostic { line: String },
    /// Emitted after every `WAIT_NOTICE_SECS` of total silence from codex.
    Waiting { seconds: u64 },
}

/// Executes one bounded, read-only Codex turn for deterministic application
/// features that need model synthesis but no tools. The prompt is provided on
/// stdin so questions and evidence do not appear in the process arguments.
/// Stdout is consumed line by line as `codex exec --json` emits it, invoking
/// `on_progress` for every recognized JSONL event so callers can surface live
/// status instead of dead air.
///
/// There is deliberately no wall-clock timeout: a hard 75s limit killed
/// legitimate slow turns through the corporate proxy. The user stops a run
/// via `cancel` instead; resolving it drops the child future, and
/// `kill_on_drop` reaps the codex process.
pub async fn run_read_only_json_with_progress(
    prompt: &str,
    on_progress: impl FnMut(SynthesisProgress) + Send,
    cancel: CancelSignal,
) -> AppResult<StructuredModelOutput> {
    run_read_only_with_images(prompt, &[], on_progress, cancel).await
}

/// Same bounded turn with image attachments (`codex exec -i`). Used by the
/// import pipeline to describe images embedded in email sources.
pub async fn run_read_only_with_images(
    prompt: &str,
    images: &[std::path::PathBuf],
    mut on_progress: impl FnMut(SynthesisProgress) + Send,
    cancel: CancelSignal,
) -> AppResult<StructuredModelOutput> {
    if prompt.chars().count() > MAX_PROMPT_CHARS {
        return Err(io_error(
            "structured model prompt exceeds 64,000 characters",
        ));
    }

    let work_dir = std::env::temp_dir().join("agentic-os-structured-model");
    std::fs::create_dir_all(&work_dir)?;

    let binary = super::resolve_binary("codex");
    let prompt = prompt.as_bytes().to_vec();
    let images = images.to_vec();
    let run = async move {
        let mut command = Command::new(&binary);
        command
            .arg("exec")
            .arg("--json")
            .arg("--skip-git-repo-check")
            .arg("-s")
            .arg("read-only");
        for image in &images {
            command.arg("-i").arg(image);
        }
        command
            // This call is a bounded, evidence-constrained JSON extraction,
            // not open-ended reasoning — it must NOT inherit the user's
            // interactive default (often "xhigh" for deep coding work),
            // which turns a seconds-long turn into minutes (observed:
            // "explain me the sierra admin API" against imported document
            // evidence). "medium" keeps synthesis quality for genuine
            // explanations at a fraction of the latency.
            .arg("-c")
            .arg(r#"model_reasoning_effort="medium""#)
            // Structured turns are tool-less by contract, but the read-only
            // sandbox does NOT gate MCP tool calls: any servers configured
            // in ~/.codex/config.toml (Jira, Bitbucket, …) would be loaded
            // and callable. Disabling them here keeps Ask/lint/vision turns
            // hermetic and avoids their startup latency on every turn.
            .arg("-c")
            .arg("mcp_servers={}")
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
        on_progress(SynthesisProgress::ProcessSpawned);
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| io_error("AI synthesis stdin is unavailable"))?;
        stdin.write_all(&prompt).await?;
        stdin.shutdown().await?;
        // poll_shutdown on ChildStdin only flushes — it does NOT close the
        // pipe fd. `codex exec` reads the stdin prompt until EOF, so without
        // this explicit drop it waits forever and never starts the turn
        // (observed live: "Synthesis process launched" then 900s+ of
        // silence). Stdin stays the transport, rather than argv like the
        // Runner uses, so prompts never appear in `ps` output.
        drop(stdin);

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io_error("AI synthesis stdout is unavailable"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| io_error("AI synthesis stderr is unavailable"))?;

        // `take` caps total bytes exactly like the previous buffered read;
        // the counter below detects when the cap truncated the stream.
        let mut stdout_lines = BufReader::new(stdout.take(MAX_STDOUT_BYTES + 1)).lines();
        // stderr is never take-capped: past the cap lines are drained and
        // discarded instead, so a chatty codex cannot fill the pipe and
        // deadlock against a reader that stopped.
        let mut stderr_lines = BufReader::new(stderr).lines();

        let mut stdout_bytes: u64 = 0;
        let mut stderr_bytes: u64 = 0;
        let mut stderr_tail: Vec<String> = Vec::new();
        let mut silent_secs: u64 = 0;
        let mut accumulator = JsonlAccumulator::default();
        let mut stdout_done = false;
        let mut stderr_done = false;

        while !stdout_done || !stderr_done {
            tokio::select! {
                line = stdout_lines.next_line(), if !stdout_done => match line? {
                    Some(line) => {
                        silent_secs = 0;
                        stdout_bytes += line.len() as u64 + 1;
                        if let Some(progress) = accumulator.ingest(&line) {
                            on_progress(progress);
                        }
                    }
                    None => stdout_done = true,
                },
                line = stderr_lines.next_line(), if !stderr_done => match line? {
                    Some(line) => {
                        silent_secs = 0;
                        stderr_bytes += line.len() as u64 + 1;
                        let trimmed = line.trim();
                        if !trimmed.is_empty() && stderr_bytes <= MAX_STDERR_BYTES {
                            stderr_tail.push(trimmed.to_string());
                            if stderr_tail.len() > 20 {
                                stderr_tail.remove(0);
                            }
                            on_progress(SynthesisProgress::Diagnostic {
                                line: trimmed.chars().take(MAX_DIAGNOSTIC_CHARS).collect(),
                            });
                        }
                    }
                    None => stderr_done = true,
                },
                _ = tokio::time::sleep(std::time::Duration::from_secs(WAIT_NOTICE_SECS)) => {
                    silent_secs += WAIT_NOTICE_SECS;
                    on_progress(SynthesisProgress::Waiting { seconds: silent_secs });
                }
            }
        }
        let status = child.wait().await?;

        if stdout_bytes > MAX_STDOUT_BYTES {
            return Err(io_error(
                "AI synthesis output exceeded the 2 MiB safety limit",
            ));
        }
        if !status.success() {
            let detail = stderr_tail.join("\n");
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

        accumulator.finish()
    };

    // A dropped sender (registry cleanup, app teardown) must not read as a
    // stop request: only an explicit `true` cancels; otherwise wait forever
    // on a future that never resolves so `run` proceeds undisturbed.
    let mut cancel = cancel;
    let cancelled = async move {
        loop {
            if *cancel.borrow() {
                break;
            }
            if cancel.changed().await.is_err() {
                std::future::pending::<()>().await;
            }
        }
    };

    tokio::select! {
        _ = cancelled => Err(io_error(STOPPED_BY_USER)),
        result = run => result,
    }
}

/// Incremental replacement for the old whole-buffer JSONL parse: each line is
/// ingested as it arrives, mapping to an optional progress marker, and the
/// final answer/usage/failure state is resolved once the stream ends.
#[derive(Debug, Default)]
struct JsonlAccumulator {
    final_message: Option<String>,
    tokens: Option<i64>,
    failure: Option<String>,
}

impl JsonlAccumulator {
    fn ingest(&mut self, line: &str) -> Option<SynthesisProgress> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return None;
        }
        let value = serde_json::from_str::<Value>(trimmed).ok()?;
        match value
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or_default()
        {
            "thread.started" => Some(SynthesisProgress::SessionStarted),
            "turn.started" => Some(SynthesisProgress::TurnStarted),
            event_type @ ("item.started" | "item.updated" | "item.completed") => {
                let item = value.get("item").unwrap_or(&Value::Null);
                match item.get("type").and_then(Value::as_str) {
                    Some("agent_message") => {
                        if event_type == "item.completed" {
                            if let Some(message) = item.get("text").and_then(Value::as_str) {
                                self.final_message = Some(message.to_string());
                            }
                            Some(SynthesisProgress::AnswerDrafted)
                        } else {
                            None
                        }
                    }
                    Some("reasoning") => Some(SynthesisProgress::Reasoning),
                    _ => None,
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
                    self.tokens = Some(input + output);
                    Some(SynthesisProgress::TokensUsed {
                        tokens: input + output,
                    })
                } else {
                    None
                }
            }
            "turn.failed" | "error" => {
                self.failure = value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
                    .or_else(|| value.get("message").and_then(Value::as_str))
                    .map(str::to_string);
                None
            }
            _ => None,
        }
    }

    fn finish(self) -> AppResult<StructuredModelOutput> {
        if let Some(failure) = self.failure {
            return Err(io_error(format!("AI synthesis failed: {failure}")));
        }
        let text = self
            .final_message
            .filter(|message| !message.trim().is_empty())
            .ok_or_else(|| io_error("AI synthesis returned no final answer"))?;
        Ok(StructuredModelOutput {
            text,
            tokens: self.tokens,
        })
    }
}

fn io_error(message: impl Into<String>) -> AppError {
    AppError::Io(std::io::Error::other(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ingest_all(lines: &str) -> (JsonlAccumulator, Vec<SynthesisProgress>) {
        let mut accumulator = JsonlAccumulator::default();
        let mut progress = Vec::new();
        for line in lines.lines() {
            if let Some(event) = accumulator.ingest(line) {
                progress.push(event);
            }
        }
        (accumulator, progress)
    }

    #[test]
    fn parses_last_agent_message_and_usage() {
        let output = r#"{"type":"item.completed","item":{"type":"agent_message","text":"{\"abstained\":false}"}}
{"type":"turn.completed","usage":{"input_tokens":30,"output_tokens":12}}
"#;
        let (accumulator, _) = ingest_all(output);
        let parsed = accumulator.finish().unwrap();
        assert_eq!(parsed.text, r#"{"abstained":false}"#);
        assert_eq!(parsed.tokens, Some(42));
    }

    #[test]
    fn maps_stream_lines_to_progress_markers() {
        let output = r#"{"type":"thread.started","thread_id":"t1"}
{"type":"turn.started"}
{"type":"item.started","item":{"type":"reasoning"}}
not-json noise line
{"type":"item.completed","item":{"type":"agent_message","text":"answer"}}
{"type":"turn.completed","usage":{"input_tokens":10,"output_tokens":5}}
"#;
        let (accumulator, progress) = ingest_all(output);
        assert_eq!(
            progress,
            vec![
                SynthesisProgress::SessionStarted,
                SynthesisProgress::TurnStarted,
                SynthesisProgress::Reasoning,
                SynthesisProgress::AnswerDrafted,
                SynthesisProgress::TokensUsed { tokens: 15 },
            ]
        );
        assert_eq!(accumulator.finish().unwrap().text, "answer");
    }

    #[test]
    fn failure_event_wins_over_message() {
        let output = r#"{"type":"item.completed","item":{"type":"agent_message","text":"partial"}}
{"type":"turn.failed","error":{"message":"proxy timeout"}}
"#;
        let (accumulator, _) = ingest_all(output);
        let error = accumulator.finish().unwrap_err().to_string();
        assert!(error.contains("proxy timeout"));
    }
}
