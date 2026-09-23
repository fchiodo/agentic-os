use rusqlite::{params, OptionalExtension};
use serde_json::Value;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::db::Db;
use crate::error::{AppError, AppResult};

use super::{MemoryOperationRecord, MemoryRecoveryReport};

fn operation_error(message: impl Into<String>) -> AppError {
    AppError::Io(std::io::Error::other(message.into()))
}

pub fn begin(db: &Db, kind: &str, entity_id: &str, payload: &Value) -> AppResult<String> {
    super::index::ensure_tables(db)?;
    let existing = db.with_conn(|conn| {
        conn.query_row(
            "SELECT id FROM memory_operations
             WHERE kind = ?1 AND entity_id = ?2 AND status = 'active'
             ORDER BY started_at DESC LIMIT 1",
            params![kind, entity_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(Into::into)
    })?;
    if existing.is_some() {
        return Err(operation_error(format!(
            "unfinished {kind} operation already exists for {entity_id}"
        )));
    }

    let id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let payload_json = serde_json::to_string(payload)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO memory_operations (
                id, kind, entity_id, stage, status, payload_json, error,
                started_at, updated_at, completed_at
             ) VALUES (?1,?2,?3,'prepared','active',?4,NULL,?5,?5,NULL)",
            params![id, kind, entity_id, payload_json, now],
        )?;
        Ok(())
    })?;
    Ok(id)
}

pub fn advance(db: &Db, operation_id: &str, stage: &str) -> AppResult<()> {
    let changed = db.with_conn(|conn| {
        conn.execute(
            "UPDATE memory_operations
             SET stage = ?1, updated_at = ?2
             WHERE id = ?3 AND status = 'active'",
            params![stage, chrono::Utc::now().to_rfc3339(), operation_id],
        )
        .map_err(Into::into)
    })?;
    if changed != 1 {
        return Err(operation_error("memory operation is no longer active"));
    }
    Ok(())
}

pub fn complete(db: &Db, operation_id: &str) -> AppResult<()> {
    finish(db, operation_id, "completed", "completed", None)
}

pub fn rolled_back(db: &Db, operation_id: &str, error: Option<&str>) -> AppResult<()> {
    finish(db, operation_id, "rolled_back", "rolled_back", error)
}

fn needs_attention(db: &Db, operation_id: &str, message: &str) -> AppResult<()> {
    finish(
        db,
        operation_id,
        "needs_attention",
        "needs_attention",
        Some(message),
    )
}

fn finish(
    db: &Db,
    operation_id: &str,
    stage: &str,
    status: &str,
    error: Option<&str>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE memory_operations
             SET stage = ?1, status = ?2, error = ?3, updated_at = ?4, completed_at = ?4
             WHERE id = ?5",
            params![stage, status, error, now, operation_id],
        )?;
        Ok(())
    })
}

pub fn list(db: &Db) -> AppResult<Vec<MemoryOperationRecord>> {
    super::index::ensure_tables(db)?;
    db.with_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT id, kind, entity_id, stage, status, error, started_at, updated_at, completed_at
             FROM memory_operations ORDER BY started_at DESC LIMIT 100",
        )?;
        let rows = statement
            .query_map([], |row| {
                Ok(MemoryOperationRecord {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    entity_id: row.get(2)?,
                    stage: row.get(3)?,
                    status: row.get(4)?,
                    error: row.get(5)?,
                    started_at: row.get(6)?,
                    updated_at: row.get(7)?,
                    completed_at: row.get(8)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

#[derive(Debug)]
struct PendingOperation {
    id: String,
    kind: String,
    entity_id: String,
    payload: Value,
    started_at: String,
}

/// Reconcile operations that were interrupted after intent was made durable.
/// Markdown remains the content source of truth. A file matching the journaled
/// proposal/import is rolled forward; an untouched base is rolled back; any
/// third state is retained as `needs_attention` rather than guessed.
pub fn recover(db: &Db) -> AppResult<MemoryRecoveryReport> {
    super::index::ensure_tables(db)?;
    let pending = db.with_conn(|conn| {
        let mut statement = conn.prepare(
            "SELECT id, kind, entity_id, payload_json, started_at
             FROM memory_operations WHERE status = 'active' ORDER BY started_at",
        )?;
        let rows = statement
            .query_map([], |row| {
                let payload: String = row.get(3)?;
                Ok(PendingOperation {
                    id: row.get(0)?,
                    kind: row.get(1)?,
                    entity_id: row.get(2)?,
                    payload: serde_json::from_str(&payload).unwrap_or(Value::Null),
                    started_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })?;

    let mut report = MemoryRecoveryReport::default();
    for operation in pending {
        let result = match operation.kind.as_str() {
            "proposal_apply" => recover_proposal(db, &operation),
            "document_import" => recover_import(db, &operation),
            other => Err(operation_error(format!(
                "unsupported memory operation kind: {other}"
            ))),
        };
        match result {
            Ok(RecoveryOutcome::Recovered) => report.recovered += 1,
            Ok(RecoveryOutcome::RolledBack) => report.rolled_back += 1,
            Err(error) => {
                let message = error.to_string();
                needs_attention(db, &operation.id, &message)?;
                report.needs_attention += 1;
                log::error!(
                    "memory recovery needs attention for {} {}: {}",
                    operation.kind,
                    operation.entity_id,
                    message
                );
            }
        }
    }
    Ok(report)
}

enum RecoveryOutcome {
    Recovered,
    RolledBack,
}

fn recover_proposal(db: &Db, operation: &PendingOperation) -> AppResult<RecoveryOutcome> {
    let proposal = super::proposals::get_by_id(db, &operation.entity_id)?
        .ok_or_else(|| operation_error("journaled proposal no longer exists"))?;
    if proposal.kind != "memory" {
        return Err(operation_error("journaled proposal is not a memory write"));
    }
    let final_status = operation
        .payload
        .get("finalStatus")
        .and_then(Value::as_str)
        .filter(|status| matches!(*status, "approved" | "auto_applied"))
        .ok_or_else(|| operation_error("journaled proposal has no valid final status"))?;
    let expected_hash = crate::audit::compute_content_hash(&proposal.new_content);
    let current = if super::vault::file_exists(&proposal.vault_path)? {
        Some(super::vault::read_file(&proposal.vault_path)?.0)
    } else {
        None
    };

    let matches_new = current
        .as_ref()
        .is_some_and(|content| crate::audit::compute_content_hash(content) == expected_hash);
    if !matches_new {
        let untouched = match proposal.op.as_str() {
            "create" | "supersede" => current.is_none(),
            "update" => current.as_ref().is_some_and(|content| {
                proposal.base_content_hash.as_deref()
                    == Some(crate::audit::compute_content_hash(content).as_str())
            }),
            _ => false,
        };
        if untouched && proposal.status == "pending" {
            rolled_back(db, &operation.id, Some("no durable file mutation found"))?;
            return Ok(RecoveryOutcome::RolledBack);
        }
        return Err(operation_error(
            "vault file matches neither the proposal nor its recorded base",
        ));
    }

    if proposal.op == "supersede" {
        repair_superseded_document(db, &proposal, &operation.started_at)?;
    }
    super::vault::git_commit(&format!(
        "mem({}): recover proposal {}",
        proposal.domain, proposal.id
    ))?;
    super::index::reindex(db)?;

    if proposal.status == "pending" {
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE memory_proposals SET status = ?1, decided_at = ?2
                 WHERE id = ?3 AND status = 'pending'",
                params![final_status, chrono::Utc::now().to_rfc3339(), proposal.id],
            )?;
            Ok(())
        })?;
    } else if proposal.status != final_status {
        return Err(operation_error(format!(
            "proposal has incompatible status {}",
            proposal.status
        )));
    }

    if !audit_exists(db, "memory_write", &proposal.id)? {
        let (frontmatter, _) = super::frontmatter::parse(&proposal.new_content)
            .ok_or_else(|| operation_error("proposal frontmatter is invalid during recovery"))?;
        crate::audit::append_row(
            db,
            proposal.task_id.as_deref().unwrap_or("memory-recovery"),
            &proposal.id,
            "memory_write",
            "Memory proposal persisted (recovered)",
            &serde_json::json!({
                "proposalId": proposal.id,
                "memoryId": frontmatter.id,
                "path": proposal.vault_path,
                "domain": proposal.domain,
                "op": proposal.op,
                "status": final_status,
                "recovered": true,
            }),
            None,
            None,
        )?;
    }
    complete(db, &operation.id)?;
    Ok(RecoveryOutcome::Recovered)
}

fn repair_superseded_document(
    db: &Db,
    proposal: &super::MemoryWriteProposal,
    started_at: &str,
) -> AppResult<()> {
    let previous_id = proposal
        .supersedes_id
        .as_deref()
        .ok_or_else(|| operation_error("supersede recovery has no previous id"))?;
    let previous = super::index::get_by_id(db, previous_id)?
        .ok_or_else(|| operation_error("superseded memory is missing from the index"))?;
    let (content, _) = super::vault::read_file(&previous.vault_path)?;
    let (mut frontmatter, body) = super::frontmatter::parse(&content)
        .ok_or_else(|| operation_error("superseded memory frontmatter is invalid"))?;
    let (next, _) = super::frontmatter::parse(&proposal.new_content)
        .ok_or_else(|| operation_error("new superseding frontmatter is invalid"))?;
    if frontmatter
        .superseded_by
        .as_deref()
        .is_some_and(|id| id != next.id)
    {
        return Err(operation_error(
            "superseded memory already points to a different replacement",
        ));
    }
    frontmatter.superseded_by = Some(next.id);
    frontmatter.valid_until = next.valid_from.or_else(|| {
        chrono::DateTime::parse_from_rfc3339(started_at)
            .ok()
            .map(|value| value.format("%Y-%m-%d").to_string())
    });
    frontmatter.updated = chrono::Utc::now().to_rfc3339();
    super::vault::write_file_atomic(
        &previous.vault_path,
        &super::frontmatter::serialize(&frontmatter, &body),
    )?;
    Ok(())
}

fn recover_import(db: &Db, operation: &PendingOperation) -> AppResult<RecoveryOutcome> {
    let payload = &operation.payload;
    let string = |key: &str| {
        payload
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| operation_error(format!("import journal is missing {key}")))
    };
    let source_path = string("sourcePath")?;
    let source_hash = string("snapshotHash")?;
    let images = payload
        .get("images")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    let path = item
                        .get("path")
                        .and_then(Value::as_str)
                        .ok_or_else(|| operation_error("import image journal is missing path"))?;
                    let content_hash = item
                        .get("contentHash")
                        .and_then(Value::as_str)
                        .ok_or_else(|| {
                            operation_error("import image journal is missing contentHash")
                        })?;
                    Ok((path.to_string(), content_hash.to_string()))
                })
                .collect::<AppResult<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    let existing_row = db.with_conn(|conn| {
        conn.query_row(
            "SELECT 1 FROM document_imports WHERE id = ?1",
            params![operation.entity_id],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(Into::into)
    })?;
    if !super::vault::file_exists(&source_path)? {
        if !existing_row {
            if let Some(original_path) = payload.get("originalPath").and_then(Value::as_str) {
                let _ = super::vault::remove_file(original_path);
            }
            for (path, _) in &images {
                let _ = super::vault::remove_file(path);
            }
            rolled_back(db, &operation.id, Some("no durable source file found"))?;
            return Ok(RecoveryOutcome::RolledBack);
        }
        return Err(operation_error(
            "import row exists but its source file is missing",
        ));
    }
    let source = super::vault::read_file(&source_path)?.0;
    if crate::audit::compute_content_hash(&source) != source_hash {
        return Err(operation_error(
            "import source snapshot does not match its journal",
        ));
    }
    if let Some(original_path) = payload.get("originalPath").and_then(Value::as_str) {
        if !super::vault::file_exists(original_path)? {
            return Err(operation_error("import original artifact is missing"));
        }
        let original = super::vault::read_bytes(original_path)?;
        let original_hash = format!("{:x}", Sha256::digest(&original));
        if original_hash != string("contentHash")? {
            return Err(operation_error(
                "import original artifact does not match its journal",
            ));
        }
    }
    for (path, expected_hash) in &images {
        if !super::vault::file_exists(path)? {
            return Err(operation_error(format!(
                "import embedded image is missing: {path}"
            )));
        }
        let bytes = super::vault::read_bytes(path)?;
        let actual_hash = format!("{:x}", Sha256::digest(&bytes));
        if &actual_hash != expected_hash {
            return Err(operation_error(format!(
                "import embedded image does not match its journal: {path}"
            )));
        }
    }

    super::vault::git_commit(&format!(
        "mem({}): recover import {}",
        string("domain")?,
        operation.entity_id
    ))?;
    if !existing_row {
        let issues = payload
            .get("extractionQualityIssues")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([]));
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO document_imports (
                    id, domain, title, input_kind, source_ref, source_path,
                    original_path, content_hash, byte_count, candidate_count, warning_count,
                    warnings_json, extraction_engine, extraction_version,
                    extraction_quality_score, extraction_quality_status,
                    extraction_quality_json, status, created_at, updated_at
                 ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,0,'[]',?10,?11,?12,?13,?14,'pending',?15,?15)",
                params![
                    operation.entity_id,
                    string("domain")?,
                    string("title")?,
                    string("inputKind")?,
                    string("sourceRef")?,
                    source_path,
                    payload.get("originalPath").and_then(Value::as_str),
                    string("contentHash")?,
                    payload.get("byteCount").and_then(Value::as_i64).unwrap_or(0),
                    payload.get("extractionEngine").and_then(Value::as_str),
                    payload.get("extractionVersion").and_then(Value::as_str),
                    payload.get("extractionQualityScore").and_then(Value::as_i64),
                    string("extractionQualityStatus")?,
                    serde_json::to_string(&issues)?,
                    string("createdAt")?,
                ],
            )?;
            Ok(())
        })?;
    }
    if !audit_exists(db, "document_import", &operation.entity_id)? {
        crate::audit::append_row(
            db,
            &format!("document-import:{}", operation.entity_id),
            &operation.entity_id,
            "document_import",
            "Document source imported (recovered)",
            &serde_json::json!({
                "importId": operation.entity_id,
                "domain": string("domain")?,
                "sourcePath": source_path,
                "imagePaths": images.iter().map(|(path, _)| path).collect::<Vec<_>>(),
                "recovered": true,
            }),
            None,
            None,
        )?;
    }
    complete(db, &operation.id)?;
    Ok(RecoveryOutcome::Recovered)
}

fn audit_exists(db: &Db, kind: &str, task_id: &str) -> AppResult<bool> {
    db.with_conn(|conn| {
        conn.query_row(
            "SELECT 1 FROM audit WHERE kind = ?1 AND task_id = ?2 LIMIT 1",
            params![kind, task_id],
            |_| Ok(true),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(Into::into)
    })
}
