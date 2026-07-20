use rusqlite::params;
use uuid::Uuid;

use crate::control_models::{ApprovalRequest, Domain, PreviewBlock, RiskLevel};
use crate::db::Db;
use crate::error::{AppError, AppResult};

pub fn create_approval(
    db: &Db,
    task_id: &str,
    domain: Domain,
    tool_name: &str,
    action_summary: &str,
    risk_level: RiskLevel,
    preview: Option<PreviewBlock>,
) -> AppResult<ApprovalRequest> {
    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let (preview_kind, preview_content) = match &preview {
        Some(p) => (Some(p.kind.clone()), Some(p.content.clone())),
        None => (None, None),
    };

    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO approvals (
                id, task_id, domain, tool_name, action_summary, risk_level,
                preview_kind, preview_content, requested_at, status, decided_at, note
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'pending',NULL,NULL)",
            params![
                id,
                task_id,
                domain.as_str(),
                tool_name,
                action_summary,
                risk_level.as_str(),
                preview_kind,
                preview_content,
                now,
            ],
        )?;
        Ok(())
    })?;

    Ok(ApprovalRequest {
        id,
        task_id: task_id.to_string(),
        task_title: fetch_task_title(db, task_id)?,
        domain,
        tool_name: tool_name.to_string(),
        action_summary: action_summary.to_string(),
        risk_level,
        preview,
        requested_at: now,
    })
}

pub fn list_pending(db: &Db) -> AppResult<Vec<ApprovalRequest>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT a.id, a.task_id, t.title, a.domain, a.tool_name, a.action_summary,
                    a.risk_level, a.preview_kind, a.preview_content, a.requested_at
             FROM approvals a
             JOIN tasks t ON t.id = a.task_id
             WHERE a.status = 'pending'
             ORDER BY a.requested_at ASC",
        )?;
        let rows = stmt
            .query_map([], row_to_approval)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

pub fn get_approval(db: &Db, id: &str) -> AppResult<Option<ApprovalRequest>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT a.id, a.task_id, t.title, a.domain, a.tool_name, a.action_summary,
                    a.risk_level, a.preview_kind, a.preview_content, a.requested_at
             FROM approvals a
             JOIN tasks t ON t.id = a.task_id
             WHERE a.id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_approval)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    })
}

/// Marks the approval decided. Returns the task_id so the caller can drive
/// the task's next transition (resume execution on approve, cancel on deny).
pub fn decide(db: &Db, id: &str, decision: &str, note: Option<&str>) -> AppResult<String> {
    if decision != "approve" && decision != "deny" {
        return Err(AppError::Io(std::io::Error::other(
            "decision must be 'approve' or 'deny'",
        )));
    }

    let status = if decision == "approve" { "approved" } else { "denied" };
    let now = chrono::Utc::now().to_rfc3339();

    let task_id = db.with_conn(|conn| {
        let task_id: String =
            conn.query_row("SELECT task_id FROM approvals WHERE id = ?1", params![id], |r| {
                r.get(0)
            })?;
        conn.execute(
            "UPDATE approvals SET status = ?1, decided_at = ?2, note = ?3 WHERE id = ?4",
            params![status, now, note, id],
        )?;
        Ok(task_id)
    })?;

    Ok(task_id)
}

fn fetch_task_title(db: &Db, task_id: &str) -> AppResult<String> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT title FROM tasks WHERE id = ?1",
            params![task_id],
            |r| r.get(0),
        )
        .map_err(Into::into)
    })
}

fn row_to_approval(row: &rusqlite::Row) -> rusqlite::Result<ApprovalRequest> {
    let preview_kind: Option<String> = row.get(7)?;
    let preview_content: Option<String> = row.get(8)?;
    let preview = match (preview_kind, preview_content) {
        (Some(kind), Some(content)) => Some(PreviewBlock { kind, content }),
        _ => None,
    };

    Ok(ApprovalRequest {
        id: row.get(0)?,
        task_id: row.get(1)?,
        task_title: row.get(2)?,
        domain: Domain::parse(&row.get::<_, String>(3)?),
        tool_name: row.get(4)?,
        action_summary: row.get(5)?,
        risk_level: parse_risk(&row.get::<_, String>(6)?),
        preview,
        requested_at: row.get(9)?,
    })
}

fn parse_risk(value: &str) -> RiskLevel {
    match value {
        "medium" => RiskLevel::Medium,
        "high" => RiskLevel::High,
        "critical" => RiskLevel::Critical,
        _ => RiskLevel::Low,
    }
}
