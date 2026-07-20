use rusqlite::params;
use serde_json::Value;
use uuid::Uuid;

use crate::control_models::{
    ArtifactRef, Domain, Harness, OriginKind, RiskLevel, StepStatus, TaskDetail, TaskEvent,
    TaskStatus, TaskStep, TaskSummary,
};
use crate::db::Db;
use crate::error::AppResult;
use crate::policy::{derive_title, PolicyDecision};

pub const DEFAULT_STEPS: [&str; 3] = [
    "Classify and check policy",
    "Run agent",
    "Record outcome",
];

pub fn create_task(
    db: &Db,
    goal: &str,
    domain: Domain,
    harness: Harness,
    agent_id: Option<String>,
    cwd: &str,
    origin: OriginKind,
    decision: &PolicyDecision,
) -> AppResult<TaskSummary> {
    let id = Uuid::new_v4().to_string();
    let title = derive_title(goal);
    let now = chrono::Utc::now().to_rfc3339();
    let status = if decision.requires_approval {
        TaskStatus::WaitingForApproval
    } else {
        TaskStatus::Planned
    };

    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO tasks (
                id, title, goal, domain, agent_id, harness, status, origin_kind,
                ontology_category_id, plan_version, current_step, step_count,
                cost_tokens, cost_usd, pending_approval_id, thread_id, sandbox_mode,
                cwd, risk_level, failure_reason, created_at, updated_at
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,NULL,1,0,?9,0,NULL,NULL,NULL,?10,?11,?12,NULL,?13,?14)",
            params![
                id,
                title,
                goal,
                domain.as_str(),
                agent_id,
                harness.as_str(),
                status.as_str(),
                origin_kind_str(origin),
                DEFAULT_STEPS.len() as i64,
                decision.sandbox_mode,
                cwd,
                decision.risk_level.as_str(),
                now,
                now,
            ],
        )?;

        for (index, title) in DEFAULT_STEPS.iter().enumerate() {
            conn.execute(
                "INSERT INTO task_steps (task_id, step_index, title, status) VALUES (?1,?2,?3,?4)",
                params![id, index as i64, title, step_status_str(StepStatus::Pending)],
            )?;
        }

        Ok(())
    })?;

    get_summary(db, &id)?.ok_or_else(|| {
        crate::error::AppError::Io(std::io::Error::other("task not found after insert"))
    })
}

pub fn list_tasks(db: &Db) -> AppResult<Vec<TaskSummary>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, goal, domain, agent_id, harness, status, origin_kind,
                    ontology_category_id, current_step, step_count, cost_tokens, cost_usd,
                    pending_approval_id, risk_level, created_at, updated_at
             FROM tasks ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], row_to_summary)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn get_summary(db: &Db, id: &str) -> AppResult<Option<TaskSummary>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, title, goal, domain, agent_id, harness, status, origin_kind,
                    ontology_category_id, current_step, step_count, cost_tokens, cost_usd,
                    pending_approval_id, risk_level, created_at, updated_at
             FROM tasks WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_summary)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    })
}

pub fn get_detail(db: &Db, id: &str) -> AppResult<Option<TaskDetail>> {
    let summary = match get_summary(db, id)? {
        Some(summary) => summary,
        None => return Ok(None),
    };

    let (plan_version, steps, artifacts, last_event_seq) = db.with_conn(|conn| {
        let plan_version: i64 =
            conn.query_row("SELECT plan_version FROM tasks WHERE id = ?1", params![id], |r| {
                r.get(0)
            })?;

        let mut step_stmt = conn.prepare(
            "SELECT step_index, title, status FROM task_steps WHERE task_id = ?1 ORDER BY step_index",
        )?;
        let steps = step_stmt
            .query_map(params![id], |row| {
                Ok(TaskStep {
                    index: row.get(0)?,
                    title: row.get(1)?,
                    status: parse_step_status(&row.get::<_, String>(2)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut artifact_stmt = conn.prepare(
            "SELECT id, label, path, kind FROM artifacts WHERE task_id = ?1",
        )?;
        let artifacts = artifact_stmt
            .query_map(params![id], |row| {
                Ok(ArtifactRef {
                    id: row.get(0)?,
                    label: row.get(1)?,
                    path: row.get(2)?,
                    kind: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let last_seq: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(seq), 0) FROM events WHERE task_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .unwrap_or(0);

        Ok((plan_version, steps, artifacts, last_seq))
    })?;

    Ok(Some(TaskDetail {
        summary,
        plan_version,
        steps,
        artifacts,
        last_event_seq,
    }))
}

pub fn set_status(db: &Db, id: &str, status: TaskStatus) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE tasks SET status = ?1, updated_at = ?2 WHERE id = ?3",
            params![status.as_str(), now, id],
        )?;
        Ok(())
    })
}

pub fn set_step_status(db: &Db, id: &str, index: i64, status: StepStatus) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE task_steps SET status = ?1 WHERE task_id = ?2 AND step_index = ?3",
            params![step_status_str(status.clone()), id, index],
        )?;
        conn.execute(
            "UPDATE tasks SET current_step = ?1, updated_at = ?2 WHERE id = ?3",
            params![index, now, id],
        )?;
        Ok(())
    })
}

pub fn set_thread_id(db: &Db, id: &str, thread_id: &str) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE tasks SET thread_id = ?1 WHERE id = ?2",
            params![thread_id, id],
        )?;
        Ok(())
    })
}

pub fn set_pending_approval(db: &Db, id: &str, approval_id: Option<&str>) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE tasks SET pending_approval_id = ?1 WHERE id = ?2",
            params![approval_id, id],
        )?;
        Ok(())
    })
}

pub fn set_failure_reason(db: &Db, id: &str, reason: &str) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE tasks SET failure_reason = ?1 WHERE id = ?2",
            params![reason, id],
        )?;
        Ok(())
    })
}

pub fn accrue_cost(db: &Db, id: &str, delta_tokens: i64) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE tasks SET cost_tokens = cost_tokens + ?1 WHERE id = ?2",
            params![delta_tokens, id],
        )?;
        Ok(())
    })
}

/// Not yet called by the Codex adapter: recognizing which files a run
/// produced needs the item-level event schema this adapter could not
/// verify end-to-end (see harness/codex.rs doc comment). Wired up as soon
/// as that schema is confirmed against a live authenticated run.
#[allow(dead_code)]
pub fn add_artifact(db: &Db, task_id: &str, label: &str, path: &str, kind: &str) -> AppResult<()> {
    let id = Uuid::new_v4().to_string();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO artifacts (id, task_id, label, path, kind) VALUES (?1,?2,?3,?4,?5)",
            params![id, task_id, label, path, kind],
        )?;
        Ok(())
    })
}

pub fn append_event(db: &Db, task_id: &str, kind: &str, payload: Value) -> AppResult<i64> {
    let ts = chrono::Utc::now().to_rfc3339();
    let payload_str = serde_json::to_string(&payload)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO events (task_id, ts, kind, payload) VALUES (?1,?2,?3,?4)",
            params![task_id, ts, kind, payload_str],
        )?;
        Ok(conn.last_insert_rowid())
    })
}

pub fn events_since(db: &Db, task_id: &str, since_seq: i64) -> AppResult<Vec<TaskEvent>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT seq, ts, kind, payload FROM events WHERE task_id = ?1 AND seq > ?2 ORDER BY seq ASC",
        )?;
        let rows = stmt
            .query_map(params![task_id, since_seq], |row| {
                let payload_str: String = row.get(3)?;
                let payload: Value = serde_json::from_str(&payload_str).unwrap_or(Value::Null);
                Ok(TaskEvent {
                    task_id: task_id.to_string(),
                    seq: row.get(0)?,
                    ts: row.get(1)?,
                    kind: row.get(2)?,
                    payload,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn running_task_count(db: &Db) -> AppResult<i64> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status IN ('running','resuming','verifying','waiting_for_tool')",
            [],
            |r| r.get(0),
        )
        .map_err(Into::into)
    })
}

pub fn spent_today_usd(db: &Db) -> AppResult<f64> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0.0) FROM tasks WHERE date(updated_at) = date('now')",
            [],
            |r| r.get(0),
        )
        .map_err(Into::into)
    })
}

fn origin_kind_str(origin: OriginKind) -> &'static str {
    match origin {
        OriginKind::Manual => "manual",
        OriginKind::Workflow => "workflow",
        OriginKind::Schedule => "schedule",
    }
}

fn step_status_str(status: StepStatus) -> &'static str {
    match status {
        StepStatus::Pending => "pending",
        StepStatus::Active => "active",
        StepStatus::Done => "done",
        StepStatus::Failed => "failed",
        StepStatus::Skipped => "skipped",
    }
}

fn parse_step_status(value: &str) -> StepStatus {
    match value {
        "active" => StepStatus::Active,
        "done" => StepStatus::Done,
        "failed" => StepStatus::Failed,
        "skipped" => StepStatus::Skipped,
        _ => StepStatus::Pending,
    }
}

fn parse_origin_kind(value: &str) -> OriginKind {
    match value {
        "workflow" => OriginKind::Workflow,
        "schedule" => OriginKind::Schedule,
        _ => OriginKind::Manual,
    }
}

fn parse_harness(value: &str) -> Harness {
    match value {
        "claude" => Harness::Claude,
        "acp" => Harness::Acp,
        _ => Harness::Codex,
    }
}

fn parse_risk(value: &str) -> RiskLevel {
    match value {
        "medium" => RiskLevel::Medium,
        "high" => RiskLevel::High,
        "critical" => RiskLevel::Critical,
        _ => RiskLevel::Low,
    }
}

fn row_to_summary(row: &rusqlite::Row) -> rusqlite::Result<TaskSummary> {
    Ok(TaskSummary {
        id: row.get(0)?,
        title: row.get(1)?,
        goal: row.get(2)?,
        domain: Domain::parse(&row.get::<_, String>(3)?),
        agent_id: row.get(4)?,
        harness: parse_harness(&row.get::<_, String>(5)?),
        status: TaskStatus::parse(&row.get::<_, String>(6)?),
        origin_kind: parse_origin_kind(&row.get::<_, String>(7)?),
        ontology_category_id: row.get(8)?,
        current_step: row.get(9)?,
        step_count: row.get(10)?,
        cost_tokens: row.get(11)?,
        cost_usd: row.get(12)?,
        pending_approval_id: row.get(13)?,
        risk_level: parse_risk(&row.get::<_, String>(14)?),
        created_at: row.get(15)?,
        updated_at: row.get(16)?,
    })
}
