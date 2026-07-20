use std::process::Stdio;

use serde_json::{json, Value};
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

use crate::audit;
use crate::control_models::{StepStatus, TaskStatus};
use crate::db::Db;
use crate::error::AppResult;
use crate::orchestrator;

pub const TASK_EVENT_CHANNEL: &str = "agent-control://task-event";

/// Spawns `codex exec --json` for the given task and streams its output
/// into the events table (live UI) and the audit table (permanent trace),
/// emitting a Tauri event per line so the Runner page updates in real time.
///
/// Schema note: the `type` discriminator on each JSONL line and the
/// `thread.started` / `turn.started` / `turn.failed` / `error` event
/// shapes below were confirmed against a live `codex exec --json` run
/// (codex-cli 0.144.4). This machine's Codex is configured against VF's
/// internal model gateway and requires VF_API_KEY, which is not available
/// in an unattended shell, so a *successful* turn with tool-call item
/// events could not be observed end-to-end while building this adapter.
/// The parser below is intentionally defensive: any `type` it does not
/// recognize is still surfaced (never silently dropped) as a generic
/// event, so it degrades gracefully rather than breaking if production
/// turns emit event shapes beyond what was observed here.
pub async fn spawn_and_stream(app: AppHandle, db: Db, task_id: String) {
    if let Err(err) = run(&app, &db, &task_id).await {
        let message = err.to_string();
        record(
            &app,
            &db,
            &task_id,
            "error",
            "output",
            "Task adapter failed",
            json!({ "message": message }),
            None,
            None,
        );
        let _ = orchestrator::set_failure_reason(&db, &task_id, &message);
        let _ = orchestrator::set_status(&db, &task_id, TaskStatus::Failed);
        emit_status(&app, &task_id, TaskStatus::Failed);
    }
}

async fn run(app: &AppHandle, db: &Db, task_id: &str) -> AppResult<()> {
    let detail = orchestrator::get_detail(db, task_id)?
        .ok_or_else(|| crate::error::AppError::Io(std::io::Error::other("task not found")))?;
    let summary = detail.summary;

    orchestrator::set_step_status(db, task_id, 0, StepStatus::Done)?;
    orchestrator::set_status(db, task_id, TaskStatus::Running)?;
    orchestrator::set_step_status(db, task_id, 1, StepStatus::Active)?;
    emit_status(app, task_id, TaskStatus::Running);

    record(
        app,
        db,
        task_id,
        "step_started",
        "input",
        "Task submitted",
        json!({ "goal": summary.goal }),
        None,
        None,
    );

    let sandbox_mode: String = db.with_conn(|conn| {
        conn.query_row(
            "SELECT sandbox_mode FROM tasks WHERE id = ?1",
            rusqlite::params![task_id],
            |r| r.get(0),
        )
        .map_err(Into::into)
    })?;
    let cwd: String = db.with_conn(|conn| {
        conn.query_row(
            "SELECT cwd FROM tasks WHERE id = ?1",
            rusqlite::params![task_id],
            |r| r.get(0),
        )
        .map_err(Into::into)
    })?;

    // Context builder (MEMORY-SPEC §7): inject domain-scoped memories as
    // data ahead of the goal, and record exactly what the agent was shown.
    let prompt = match crate::memory::context::build_memory_context(
        db,
        &summary.goal,
        summary.domain.as_str(),
    ) {
        Ok(context) if !context.prompt_block.is_empty() => {
            record(
                app,
                db,
                task_id,
                "context",
                "context",
                &format!("Injected {} memories into context", context.injected_paths.len()),
                json!({
                    "injected": context.injected_paths,
                    "unverified": context.unverified_paths,
                }),
                None,
                None,
            );
            format!("{}{}", summary.goal, context.prompt_block)
        }
        Ok(_) => summary.goal.clone(),
        Err(err) => {
            log::warn!("memory context build failed for task {task_id}: {err}");
            summary.goal.clone()
        }
    };

    let binary = super::resolve_binary("codex");
    let mut command = Command::new(&binary);
    command
        .arg("exec")
        .arg("--json")
        .arg("--skip-git-repo-check")
        .arg("-s")
        .arg(&sandbox_mode)
        .arg("-C")
        .arg(&cwd)
        .arg(&prompt)
        .current_dir(&cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = command.spawn().map_err(|err| {
        crate::error::AppError::Io(std::io::Error::other(format!(
            "failed to start '{binary} exec': {err}"
        )))
    })?;

    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let mut stdout_lines = BufReader::new(stdout).lines();
    let mut stderr_lines = BufReader::new(stderr).lines();

    let app_err = app.clone();
    let db_err = db.clone();
    let task_id_err = task_id.to_string();
    let stderr_handle = tauri::async_runtime::spawn(async move {
        while let Ok(Some(line)) = stderr_lines.next_line().await {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            record(
                &app_err,
                &db_err,
                &task_id_err,
                "agent_message",
                "output",
                trimmed,
                json!({ "stream": "stderr", "line": trimmed }),
                None,
                None,
            );
        }
    });

    let mut thread_id: Option<String> = None;
    let mut saw_failure = false;
    let mut failure_message: Option<String> = None;

    while let Ok(Some(line)) = stdout_lines.next_line().await {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        match serde_json::from_str::<Value>(trimmed) {
            Ok(value) => {
                let event_type = value
                    .get("type")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string();

                match event_type.as_str() {
                    "thread.started" => {
                        if let Some(id) = value.get("thread_id").and_then(Value::as_str) {
                            thread_id = Some(id.to_string());
                            let _ = orchestrator::set_thread_id(db, task_id, id);
                        }
                        record(
                            app, db, task_id, "agent_message", "context", "Session started",
                            value, None, None,
                        );
                    }
                    "turn.started" => {
                        record(
                            app, db, task_id, "step_started", "model_call", "Turn started", value,
                            None, None,
                        );
                    }
                    "turn.completed" => {
                        record(
                            app, db, task_id, "step_completed", "model_call", "Turn completed",
                            value, None, None,
                        );
                    }
                    "turn.failed" | "error" => {
                        saw_failure = true;
                        failure_message = extract_message(&value);
                        record(
                            app,
                            db,
                            task_id,
                            "error",
                            "output",
                            failure_message.as_deref().unwrap_or("Task failed"),
                            value,
                            None,
                            None,
                        );
                    }
                    t if is_tool_shaped(t) => {
                        let summary_text = summarize(&value, t);
                        record(
                            app, db, task_id, "tool_call", "tool_call",
                            &summary_text, value, None, None,
                        );
                    }
                    t if t.contains("token") || t.contains("usage") => {
                        let tokens = extract_token_count(&value);
                        if let Some(tokens) = tokens {
                            let _ = orchestrator::accrue_cost(db, task_id, tokens);
                        }
                        record(
                            app, db, task_id, "cost_update", "model_call", "Token usage update",
                            value, tokens, None,
                        );
                    }
                    _ => {
                        let summary_text = summarize(&value, &event_type);
                        record(
                            app,
                            db,
                            task_id,
                            "agent_message",
                            "model_call",
                            &summary_text,
                            value,
                            None,
                            None,
                        );
                    }
                }
            }
            Err(_) => {
                // Non-JSON line (e.g. codex's stdin-read notice). Never
                // dropped — surfaced as a raw agent_message so nothing is
                // silently lost from the log.
                record(
                    app,
                    db,
                    task_id,
                    "agent_message",
                    "output",
                    trimmed,
                    json!({ "raw_line": trimmed }),
                    None,
                    None,
                );
            }
        }
    }

    let _ = stderr_handle.await;
    let exit_status = child.wait().await;
    let exited_ok = exit_status.map(|status| status.success()).unwrap_or(false);

    let final_step_status = if saw_failure || !exited_ok {
        StepStatus::Failed
    } else {
        StepStatus::Done
    };
    orchestrator::set_step_status(db, task_id, 1, final_step_status)?;
    orchestrator::set_step_status(db, task_id, 2, StepStatus::Active)?;

    let final_status = if saw_failure || !exited_ok {
        TaskStatus::Failed
    } else {
        TaskStatus::Completed
    };

    if let Some(reason) = &failure_message {
        orchestrator::set_failure_reason(db, task_id, reason)?;
    } else if !exited_ok {
        orchestrator::set_failure_reason(
            db,
            task_id,
            "codex exec exited with a non-zero status and no error event was reported",
        )?;
    }

    orchestrator::set_step_status(db, task_id, 2, StepStatus::Done)?;
    orchestrator::set_status(db, task_id, final_status)?;
    emit_status(app, task_id, final_status);

    record(
        app,
        db,
        task_id,
        "status_changed",
        "output",
        &format!("Task {}", final_status.as_str()),
        json!({ "status": final_status.as_str(), "threadId": thread_id }),
        None,
        None,
    );

    // Run capture (MEMORY-SPEC §4 source 1): a successfully completed run
    // becomes an episodic memory. Failures never block task completion.
    if final_status == TaskStatus::Completed {
        match crate::memory::pipeline::process_run_capture(
            db,
            task_id,
            summary.domain.as_str(),
            &summary.title,
            &summary.goal,
            "completed",
        ) {
            Ok(Some(proposal)) => {
                record(
                    app,
                    db,
                    task_id,
                    "memory_captured",
                    "output",
                    "Run captured to memory",
                    json!({ "proposalId": proposal.id, "vaultPath": proposal.vault_path, "status": proposal.status }),
                    None,
                    None,
                );
            }
            Ok(None) => {}
            Err(err) => log::warn!("run capture failed for task {task_id}: {err}"),
        }
    }

    Ok(())
}

fn is_tool_shaped(event_type: &str) -> bool {
    event_type.contains("item")
        || event_type.contains("command")
        || event_type.contains("exec")
        || event_type.contains("patch")
        || event_type.contains("file")
}

fn extract_message(value: &Value) -> Option<String> {
    value
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(Value::as_str)
        .or_else(|| value.get("message").and_then(Value::as_str))
        .map(str::to_string)
}

fn extract_token_count(value: &Value) -> Option<i64> {
    for key in ["total_tokens", "tokens", "output_tokens", "input_tokens"] {
        if let Some(n) = value.get(key).and_then(Value::as_i64) {
            return Some(n);
        }
    }
    None
}

fn summarize(value: &Value, event_type: &str) -> String {
    value
        .get("message")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            value
                .get("command")
                .and_then(Value::as_str)
                .map(|c| format!("$ {c}"))
        })
        .unwrap_or_else(|| format!("[{event_type}]"))
}

#[allow(clippy::too_many_arguments)]
fn record(
    app: &AppHandle,
    db: &Db,
    task_id: &str,
    event_kind: &str,
    audit_kind: &str,
    summary: &str,
    detail: Value,
    tokens: Option<i64>,
    cost_usd: Option<f64>,
) {
    let seq = match orchestrator::append_event(db, task_id, event_kind, detail.clone()) {
        Ok(seq) => seq,
        Err(err) => {
            log::error!("failed to persist task event: {err}");
            return;
        }
    };

    if let Err(err) = audit::append_row(
        db, task_id, task_id, audit_kind, summary, &detail, tokens, cost_usd,
    ) {
        log::error!("failed to append audit row: {err}");
    }

    let event = crate::control_models::TaskEvent {
        task_id: task_id.to_string(),
        seq,
        ts: chrono::Utc::now().to_rfc3339(),
        kind: event_kind.to_string(),
        payload: detail,
    };

    if let Err(err) = app.emit(TASK_EVENT_CHANNEL, &event) {
        log::error!("failed to emit task event: {err}");
    }
}

fn emit_status(app: &AppHandle, task_id: &str, status: TaskStatus) {
    let event = crate::control_models::TaskEvent {
        task_id: task_id.to_string(),
        seq: -1,
        ts: chrono::Utc::now().to_rfc3339(),
        kind: "status_changed".to_string(),
        payload: json!({ "status": status.as_str() }),
    };
    let _ = app.emit(TASK_EVENT_CHANNEL, &event);
}
