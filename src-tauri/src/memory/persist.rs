use rusqlite::params;

use crate::db::Db;
use crate::error::{AppError, AppResult};

use super::{frontmatter, index, vault, MemoryFrontmatter, MemoryRow, MemoryWriteProposal};

fn row_from_document(
    path: &str,
    content: &str,
    fm: &MemoryFrontmatter,
    body: &str,
    existing: Option<&MemoryRow>,
    status: &str,
) -> MemoryRow {
    MemoryRow {
        id: fm.id.clone(),
        vault_path: path.to_string(),
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
        last_accessed_at: existing.and_then(|row| row.last_accessed_at.clone()),
        access_count: existing.map(|row| row.access_count).unwrap_or(0),
        expires_at: fm.expires.clone(),
        provenance: serde_json::to_string(&fm.provenance).unwrap_or_default(),
        content_hash: crate::audit::compute_content_hash(content),
        status: status.to_string(),
    }
}

fn restore_file(path: &str, previous: &Option<String>) {
    match previous {
        Some(content) => {
            let _ = vault::write_file_atomic(path, content);
        }
        None => {
            let _ = vault::remove_file(path);
        }
    }
}

/// Persist one approved or auto-applied memory proposal. This is the only
/// path by which proposal content reaches the vault. Cross-resource ACID is
/// not available for Git + filesystem + SQLite, so the operation uses
/// explicit compensating rollback and never suppresses an error.
pub fn apply_memory_proposal(
    db: &Db,
    proposal: &MemoryWriteProposal,
    final_status: &str,
) -> AppResult<()> {
    let _write_guard = vault::lock_writes();
    if proposal.kind != "memory" {
        return Err(AppError::Io(std::io::Error::other(
            "memory persistence received a non-memory proposal",
        )));
    }
    if !matches!(final_status, "approved" | "auto_applied") {
        return Err(AppError::Io(std::io::Error::other(
            "invalid applied proposal status",
        )));
    }
    let current_proposal = super::proposals::get_by_id(db, &proposal.id)?
        .ok_or_else(|| AppError::Io(std::io::Error::other("proposal not found")))?;
    if current_proposal.status != "pending" {
        return Err(AppError::Io(std::io::Error::other(
            "proposal is no longer pending",
        )));
    }

    let (new_fm, new_body) = frontmatter::parse(&proposal.new_content).ok_or_else(|| {
        AppError::Io(std::io::Error::other(
            "proposal contains invalid memory frontmatter",
        ))
    })?;
    if new_fm.domain != proposal.domain
        || !proposal
            .vault_path
            .starts_with(&format!("{}/", proposal.domain))
    {
        return Err(AppError::Io(std::io::Error::other(
            "proposal violates its domain fence",
        )));
    }

    vault::ensure_vault()?;
    let previous_target = if vault::file_exists(&proposal.vault_path)? {
        Some(vault::read_file(&proposal.vault_path)?.0)
    } else {
        None
    };
    let previous_new_row = index::get_by_id(db, &new_fm.id)?;
    match proposal.op.as_str() {
        "create" if previous_target.is_some() || previous_new_row.is_some() => {
            return Err(AppError::Io(std::io::Error::other(
                "memory changed after proposal creation; regenerate the proposal",
            )));
        }
        "update" => {
            let current_content = previous_target.as_ref().ok_or_else(|| {
                AppError::Io(std::io::Error::other(
                    "memory update target is missing; regenerate the proposal",
                ))
            })?;
            let expected = proposal.base_content_hash.as_ref().ok_or_else(|| {
                AppError::Io(std::io::Error::other(
                    "memory update base is missing; regenerate the proposal",
                ))
            })?;
            if crate::audit::compute_content_hash(current_content) != *expected {
                return Err(AppError::Io(std::io::Error::other(
                    "memory changed after proposal creation; regenerate the proposal",
                )));
            }
        }
        "supersede" | "create" => {}
        _ => {
            return Err(AppError::Io(std::io::Error::other(
                "invalid memory proposal operation",
            )));
        }
    }

    // A supersede creates the new file and closes the truth window in the
    // old file. Updating YAML as well as SQLite makes the state survive a
    // complete index rebuild.
    let mut superseded_snapshot: Option<(String, String, MemoryRow)> = None;
    let mut superseded_document: Option<(String, String, MemoryFrontmatter, String)> = None;
    if proposal.op == "supersede" {
        let old_id = proposal.supersedes_id.as_deref().ok_or_else(|| {
            AppError::Io(std::io::Error::other("supersede proposal has no target"))
        })?;
        let old_row = index::get_by_id(db, old_id)?
            .ok_or_else(|| AppError::Io(std::io::Error::other("superseded memory not found")))?;
        if old_row.domain != proposal.domain || old_row.id == new_fm.id {
            return Err(AppError::Io(std::io::Error::other(
                "invalid supersede target",
            )));
        }
        let (old_content, _) = vault::read_file(&old_row.vault_path)?;
        let expected = proposal.base_content_hash.as_ref().ok_or_else(|| {
            AppError::Io(std::io::Error::other(
                "supersede base is missing; regenerate the proposal",
            ))
        })?;
        if crate::audit::compute_content_hash(&old_content) != *expected {
            return Err(AppError::Io(std::io::Error::other(
                "superseded memory changed after proposal creation; regenerate the proposal",
            )));
        }
        let (mut old_fm, old_body) = frontmatter::parse(&old_content).ok_or_else(|| {
            AppError::Io(std::io::Error::other(
                "superseded memory has invalid frontmatter",
            ))
        })?;
        old_fm.valid_until = new_fm
            .valid_from
            .clone()
            .or_else(|| Some(chrono::Utc::now().format("%Y-%m-%d").to_string()));
        old_fm.updated = chrono::Utc::now().to_rfc3339();
        let replacement = frontmatter::serialize(&old_fm, &old_body);
        superseded_snapshot = Some((old_row.vault_path.clone(), old_content, old_row));
        superseded_document = Some((old_id.to_string(), replacement, old_fm, old_body));
    }

    let result = (|| -> AppResult<()> {
        vault::write_file_atomic(&proposal.vault_path, &proposal.new_content)?;
        if let Some((_, replacement, old_fm, _)) = &superseded_document {
            let old_path = superseded_snapshot
                .as_ref()
                .map(|snapshot| snapshot.0.as_str())
                .ok_or_else(|| AppError::Io(std::io::Error::other("missing supersede snapshot")))?;
            debug_assert_eq!(old_fm.domain, proposal.domain);
            vault::write_file_atomic(old_path, replacement)?;
        }

        vault::git_commit(&format!(
            "mem({}): {} {} [{}]",
            proposal.domain,
            proposal.op,
            proposal
                .vault_path
                .rsplit('/')
                .next()
                .unwrap_or("memory")
                .trim_end_matches(".md"),
            new_fm.provenance.source,
        ))?;

        let new_row = row_from_document(
            &proposal.vault_path,
            &proposal.new_content,
            &new_fm,
            &new_body,
            previous_new_row.as_ref(),
            "active",
        );
        index::upsert(db, &new_row, &new_body, &new_fm.tags)?;

        if let Some((old_id, replacement, old_fm, old_body)) = &superseded_document {
            let old_row = superseded_snapshot
                .as_ref()
                .map(|snapshot| &snapshot.2)
                .ok_or_else(|| AppError::Io(std::io::Error::other("missing supersede row")))?;
            let stale_row = row_from_document(
                &old_row.vault_path,
                replacement,
                old_fm,
                old_body,
                Some(old_row),
                "stale",
            );
            debug_assert_eq!(&stale_row.id, old_id);
            index::upsert(db, &stale_row, old_body, &old_fm.tags)?;
        }

        let decided_at = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            let changed = conn.execute(
                "UPDATE memory_proposals SET status = ?1, decided_at = ?2 WHERE id = ?3 AND status = 'pending'",
                params![final_status, decided_at, proposal.id],
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
            "memory_write",
            "Memory proposal persisted",
            &serde_json::json!({
                "proposalId": proposal.id,
                "memoryId": new_fm.id,
                "path": proposal.vault_path,
                "domain": proposal.domain,
                "op": proposal.op,
                "status": final_status,
                "supersedesId": proposal.supersedes_id,
            }),
            None,
            None,
        )?;

        Ok(())
    })();

    if let Err(error) = result {
        restore_file(&proposal.vault_path, &previous_target);
        if let Some((old_path, old_content, _)) = &superseded_snapshot {
            let _ = vault::write_file_atomic(old_path, old_content);
        }

        match (&previous_new_row, &previous_target) {
            (Some(row), Some(content)) => {
                if let Some((fm, body)) = frontmatter::parse(content) {
                    let _ = index::upsert(db, row, &body, &fm.tags);
                }
            }
            (None, _) => {
                let _ = index::remove(db, &new_fm.id);
            }
            _ => {}
        }
        if let Some((_, old_content, old_row)) = &superseded_snapshot {
            if let Some((fm, body)) = frontmatter::parse(old_content) {
                let _ = index::upsert(db, old_row, &body, &fm.tags);
            }
        }
        let _ = db.with_conn(|conn| {
            conn.execute(
                "UPDATE memory_proposals SET status = 'pending', decided_at = NULL WHERE id = ?1",
                params![proposal.id],
            )?;
            Ok(())
        });
        let _ = vault::git_commit(&format!(
            "mem({}): rollback proposal {}",
            proposal.domain, proposal.id
        ));
        return Err(error);
    }

    Ok(())
}
