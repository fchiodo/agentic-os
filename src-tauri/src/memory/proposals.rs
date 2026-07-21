use crate::error::AppError;
use rusqlite::params;

use crate::db::Db;
use crate::error::AppResult;

use super::vault;
use super::{MemoryWriteProposal, ProposalStatus};

/// List proposals, optionally filtered by status.
pub fn list(db: &Db, status_filter: Option<&str>) -> AppResult<Vec<MemoryWriteProposal>> {
    super::index::ensure_tables(db)?;
    if status_filter.is_some_and(|status| {
        !matches!(
            status,
            "pending" | "approved" | "discarded" | "auto_applied"
        )
    }) {
        return Err(AppError::Io(std::io::Error::other(
            "invalid proposal status filter",
        )));
    }
    db.with_conn(|conn| {
        let (sql, param_values): (String, Vec<String>) = match status_filter {
            Some(s) => (
                "SELECT id, task_id, vault_path, domain, kind, op, supersedes_id,
                        sensitivity, unified_diff, new_content, provenance,
                        gate_report, requires_approval, status, created_at, decided_at,
                        base_content_hash
                 FROM memory_proposals WHERE status = ?1 ORDER BY created_at DESC"
                    .to_string(),
                vec![s.to_string()],
            ),
            None => (
                "SELECT id, task_id, vault_path, domain, kind, op, supersedes_id,
                        sensitivity, unified_diff, new_content, provenance,
                        gate_report, requires_approval, status, created_at, decided_at,
                        base_content_hash
                 FROM memory_proposals ORDER BY created_at DESC"
                    .to_string(),
                vec![],
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let rows = stmt
            .query_map(param_refs.as_slice(), row_to_proposal)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Decide on a proposal: approve or discard.
pub fn decide(db: &Db, id: &str, decision: &str) -> AppResult<MemoryWriteProposal> {
    super::index::ensure_tables(db)?;
    if !matches!(decision, "approve" | "discard") {
        return Err(AppError::Io(std::io::Error::other(
            "decision must be approve or discard",
        )));
    }

    let proposal = get_by_id(db, id)?
        .ok_or_else(|| AppError::Io(std::io::Error::other("proposal not found")))?;

    if proposal.status != ProposalStatus::Pending.as_str() {
        return Err(AppError::Io(std::io::Error::other(
            "proposal is not pending",
        )));
    }

    let now = chrono::Utc::now().to_rfc3339();

    if decision == "approve" {
        if proposal.kind == "skill" {
            // Distilled skills land in the harness skills directory, not
            // the vault (MEMORY-SPEC §4 source 4). No index row: skills
            // are procedural memory owned by the harnesses; the Catalog
            // scanner picks them up with provenance in the frontmatter.
            let _write_guard = vault::lock_writes();
            let previous = vault::read_skill_file(&proposal.vault_path).ok();
            let current = get_by_id(db, id)?.ok_or_else(|| {
                AppError::Io(std::io::Error::other("proposal not found"))
            })?;
            if current.status != ProposalStatus::Pending.as_str() {
                return Err(AppError::Io(std::io::Error::other(
                    "proposal is no longer pending",
                )));
            }
            match (proposal.op.as_str(), previous.as_ref(), proposal.base_content_hash.as_ref()) {
                ("create", Some(_), _) => {
                    return Err(AppError::Io(std::io::Error::other(
                        "skill changed after proposal creation; regenerate the proposal",
                    )));
                }
                ("update", Some(content), Some(expected))
                    if crate::audit::compute_content_hash(content) != *expected =>
                {
                    return Err(AppError::Io(std::io::Error::other(
                        "skill changed after proposal creation; regenerate the proposal",
                    )));
                }
                ("update", None, _) | ("update", _, None) => {
                    return Err(AppError::Io(std::io::Error::other(
                        "skill update base is missing; regenerate the proposal",
                    )));
                }
                _ => {}
            }
            vault::write_skill_file(&proposal.vault_path, &proposal.new_content)?;

            let apply_result = (|| -> AppResult<()> {
                db.with_conn(|conn| {
                    let changed = conn.execute(
                        "UPDATE memory_proposals SET status = 'approved', decided_at = ?1 WHERE id = ?2",
                        params![now, id],
                    )?;
                    if changed != 1 {
                        return Err(AppError::Io(std::io::Error::other(
                            "proposal was decided concurrently",
                        )));
                    }
                    Ok(())
                })?;

                crate::audit::append_row(
                    db,
                    proposal.task_id.as_deref().unwrap_or("memory"),
                    &proposal.id,
                    "skill_write",
                    "Distilled skill approved",
                    &serde_json::json!({
                        "proposalId": proposal.id,
                        "path": proposal.vault_path,
                        "domain": proposal.domain,
                    }),
                    None,
                    None,
                )
            })();

            if let Err(error) = apply_result {
                if let Some(previous) = previous {
                    let _ = vault::write_skill_file(&proposal.vault_path, &previous);
                } else {
                    let _ = vault::remove_skill_file(&proposal.vault_path);
                }
                let _ = db.with_conn(|conn| {
                    conn.execute(
                        "UPDATE memory_proposals SET status = 'pending', decided_at = NULL WHERE id = ?1",
                        params![id],
                    )?;
                    Ok(())
                });
                return Err(error);
            }

            return get_by_id(db, id)?
                .ok_or_else(|| AppError::Io(std::io::Error::other("proposal not found")));
        }

        super::persist::apply_memory_proposal(db, &proposal, "approved")?;
    } else {
        // Discard
        let _write_guard = vault::lock_writes();
        let current = get_by_id(db, id)?
            .ok_or_else(|| AppError::Io(std::io::Error::other("proposal not found")))?;
        if current.status != ProposalStatus::Pending.as_str() {
            return Err(AppError::Io(std::io::Error::other(
                "proposal is no longer pending",
            )));
        }
        db.with_conn(|conn| {
            let changed = conn.execute(
                "UPDATE memory_proposals SET status = 'discarded', decided_at = ?1 WHERE id = ?2 AND status = 'pending'",
                params![now, id],
            )?;
            if changed != 1 {
                return Err(AppError::Io(std::io::Error::other(
                    "proposal was decided concurrently",
                )));
            }
            Ok(())
        })?;
        if let Err(error) = crate::audit::append_row(
            db,
            proposal.task_id.as_deref().unwrap_or("memory"),
            &proposal.id,
            "memory_proposal",
            "Memory proposal discarded",
            &serde_json::json!({"proposalId": proposal.id, "path": proposal.vault_path}),
            None,
            None,
        ) {
            let _ = db.with_conn(|conn| {
                conn.execute(
                    "UPDATE memory_proposals SET status = 'pending', decided_at = NULL WHERE id = ?1",
                    params![id],
                )?;
                Ok(())
            });
            return Err(error);
        }
    }

    get_by_id(db, id)?.ok_or_else(|| AppError::Io(std::io::Error::other("proposal not found")))
}

/// Get a single proposal by ID.
pub fn get_by_id(db: &Db, id: &str) -> AppResult<Option<MemoryWriteProposal>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, task_id, vault_path, domain, kind, op, supersedes_id,
                    sensitivity, unified_diff, new_content, provenance,
                    gate_report, requires_approval, status, created_at, decided_at,
                    base_content_hash
             FROM memory_proposals WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_proposal)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    })
}

fn row_to_proposal(row: &rusqlite::Row) -> rusqlite::Result<MemoryWriteProposal> {
    Ok(MemoryWriteProposal {
        id: row.get(0)?,
        task_id: row.get(1)?,
        vault_path: row.get(2)?,
        domain: row.get(3)?,
        kind: row.get(4)?,
        op: row.get(5)?,
        supersedes_id: row.get(6)?,
        sensitivity: row.get(7)?,
        unified_diff: row.get(8)?,
        new_content: row.get(9)?,
        provenance: row.get(10)?,
        gate_report: row.get(11)?,
        requires_approval: row.get::<_, i64>(12)? != 0,
        status: row.get(13)?,
        created_at: row.get(14)?,
        decided_at: row.get(15)?,
        base_content_hash: row.get(16)?,
    })
}
