use crate::error::AppError;
use rusqlite::params;

use crate::db::Db;
use crate::error::AppResult;

use super::vault;
use super::{MemoryRow, MemoryWriteProposal, ProposalStatus};

/// List proposals, optionally filtered by status.
pub fn list(db: &Db, status_filter: Option<&str>) -> AppResult<Vec<MemoryWriteProposal>> {
    super::index::ensure_tables(db)?;
    db.with_conn(|conn| {
        let (sql, param_values): (String, Vec<String>) = match status_filter {
            Some(s) => (
                "SELECT id, task_id, vault_path, domain, kind, op, supersedes_id,
                        sensitivity, unified_diff, new_content, provenance,
                        gate_report, requires_approval, status, created_at, decided_at
                 FROM memory_proposals WHERE status = ?1 ORDER BY created_at DESC"
                    .to_string(),
                vec![s.to_string()],
            ),
            None => (
                "SELECT id, task_id, vault_path, domain, kind, op, supersedes_id,
                        sensitivity, unified_diff, new_content, provenance,
                        gate_report, requires_approval, status, created_at, decided_at
                 FROM memory_proposals ORDER BY created_at DESC"
                    .to_string(),
                vec![],
            ),
        };

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|v| v as &dyn rusqlite::types::ToSql).collect();
        let rows = stmt
            .query_map(param_refs.as_slice(), row_to_proposal)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Decide on a proposal: approve or discard.
pub fn decide(db: &Db, id: &str, decision: &str) -> AppResult<MemoryWriteProposal> {
    super::index::ensure_tables(db)?;

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
            vault::write_skill_file(&proposal.vault_path, &proposal.new_content)?;

            db.with_conn(|conn| {
                conn.execute(
                    "UPDATE memory_proposals SET status = 'approved', decided_at = ?1 WHERE id = ?2",
                    params![now, id],
                )?;
                Ok(())
            })?;

            return get_by_id(db, id)?
                .ok_or_else(|| AppError::Io(std::io::Error::other("proposal not found")));
        }

        // Write the file + git commit + upsert index
        vault::ensure_vault()?;
        vault::write_file(&proposal.vault_path, &proposal.new_content)?;
        let _ = vault::git_commit(&format!(
            "mem({}): {} [proposal:{}]",
            proposal.domain, proposal.op, proposal.id
        ));

        // Parse frontmatter from the new content to build index row
        if let Some((fm, body)) = super::frontmatter::parse(&proposal.new_content) {
            let content_hash =
                crate::audit::compute_content_hash(&proposal.new_content);
            let row = MemoryRow {
                id: fm.id.clone(),
                vault_path: proposal.vault_path.clone(),
                domain: fm.domain.clone(),
                mem_type: fm.mem_type.as_str().to_string(),
                title: fm.title.clone(),
                summary: Some(body.chars().take(280).collect()),
                sensitivity: fm.sensitivity.as_str().to_string(),
                confidence: fm.confidence,
                created_at: fm.created.clone(),
                updated_at: fm.updated.clone(),
                valid_from: fm.valid_from.clone(),
                valid_until: fm.valid_until.clone(),
                stale_after_days: fm.stale_after_days,
                last_confirmed_at: fm.last_confirmed.clone(),
                confirmation_count: fm.confirmations.unwrap_or(0),
                last_accessed_at: None,
                access_count: 0,
                expires_at: fm.expires.clone(),
                provenance: serde_json::to_string(&fm.provenance).unwrap_or_default(),
                content_hash,
                status: "active".to_string(),
            };
            let _ = super::index::upsert(db, &row, &body, &fm.tags);
        }

        // If supersede: the old memory keeps existing but its truth window
        // closes (valid_until = now) and it drops to stale (§5.3 — nothing
        // is destroyed, truth is versioned).
        if let Some(ref supersedes_id) = proposal.supersedes_id {
            if proposal.op == "supersede" {
                let _ = db.with_conn(|conn| {
                    conn.execute(
                        "UPDATE memories SET status = 'stale', valid_until = ?1, updated_at = ?1 WHERE id = ?2",
                        params![now, supersedes_id],
                    )?;
                    Ok::<_, crate::error::AppError>(())
                });
            }
        }

        db.with_conn(|conn| {
            conn.execute(
                "UPDATE memory_proposals SET status = 'approved', decided_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            Ok(())
        })?;
    } else {
        // Discard
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE memory_proposals SET status = 'discarded', decided_at = ?1 WHERE id = ?2",
                params![now, id],
            )?;
            Ok(())
        })?;
    }

    get_by_id(db, id)?.ok_or_else(|| AppError::Io(std::io::Error::other("proposal not found")))
}

/// Get a single proposal by ID.
pub fn get_by_id(db: &Db, id: &str) -> AppResult<Option<MemoryWriteProposal>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, task_id, vault_path, domain, kind, op, supersedes_id,
                    sensitivity, unified_diff, new_content, provenance,
                    gate_report, requires_approval, status, created_at, decided_at
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
    })
}
