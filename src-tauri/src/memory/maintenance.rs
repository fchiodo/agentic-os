use rusqlite::params;

use crate::db::Db;
use crate::error::AppResult;

use super::vault;
use super::MaintenanceResult;

/// Parse a stored date that may be RFC 3339 ("2026-07-20T09:12:00+00:00")
/// or date-only ("2026-07-20"). Frontmatter and DB rows contain both
/// shapes, so the sweep must accept both — parsing only %Y-%m-%d silently
/// skips every RFC 3339 row and staleness never fires.
fn parse_date_flexible(value: &str) -> Option<chrono::NaiveDate> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(value) {
        return Some(dt.date_naive());
    }
    chrono::NaiveDate::parse_from_str(&value[..value.len().min(10)], "%Y-%m-%d").ok()
}

/// Run the maintenance sweep: expire episodes past TTL, mark stale facts.
pub fn run_sweep(db: &Db) -> AppResult<MaintenanceResult> {
    let _write_guard = vault::lock_writes();
    super::index::ensure_tables(db)?;
    let now_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // 1. TTL sweep: episodes past expires_at → expired, move to archive.
    // substr(...,1,10) normalizes RFC 3339 timestamps to their date part so
    // the lexicographic comparison stays correct for both stored shapes.
    let expiry_rows = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, vault_path, domain, title FROM memories
             WHERE mem_type = 'episode' AND expires_at IS NOT NULL AND substr(expires_at, 1, 10) < ?1 AND status != 'expired'",
        )?;

        let rows = stmt
            .query_map(params![now_date], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, crate::error::AppError>(rows)
    })?;

    let mut expired = 0i64;
    for (id, path, domain, title) in expiry_rows {
        let archived_path = vault::archive_file(&path, &domain)?;
        let root = vault::vault_root()?;
        let relative_archive = archived_path
            .strip_prefix(&root)
            .map_err(|_| {
                crate::error::AppError::Io(std::io::Error::other(
                    "archive path escaped vault root",
                ))
            })?
            .to_string_lossy()
            .to_string();

        if let Err(error) = vault::git_commit(&format!("mem({domain}): expire {title}")) {
            let _ = vault::restore_archived_file(&relative_archive, &path);
            return Err(error);
        }

        let index_result = db.with_conn(|conn| {
            let transaction = conn.unchecked_transaction()?;
            transaction.execute(
                "UPDATE memories SET status = 'expired', vault_path = ?2, updated_at = ?3 WHERE id = ?1",
                params![id, relative_archive, chrono::Utc::now().to_rfc3339()],
            )?;
            transaction.execute(
                "DELETE FROM memories_fts WHERE rowid IN (SELECT rowid FROM memories WHERE id = ?1)",
                params![id],
            )?;
            transaction.commit()?;
            Ok(())
        });
        if let Err(error) = index_result {
            let _ = vault::restore_archived_file(&relative_archive, &path);
            let _ = vault::git_commit(&format!("mem({domain}): rollback expiry {title}"));
            return Err(error);
        }

        if let Err(error) = crate::audit::append_row(
            db,
            "memory-maintenance",
            &id,
            "memory_write",
            "Episode archived after TTL",
            &serde_json::json!({"id": id, "from": path, "to": relative_archive}),
            None,
            None,
        ) {
            let _ = db.with_conn(|conn| {
                let transaction = conn.unchecked_transaction()?;
                transaction.execute(
                    "UPDATE memories SET status = 'active', vault_path = ?2 WHERE id = ?1",
                    params![id, path],
                )?;
                transaction.commit()?;
                Ok(())
            });
            let _ = vault::restore_archived_file(&relative_archive, &path);
            let _ = vault::git_commit(&format!("mem({domain}): rollback expiry {title}"));
            // Rebuild the removed FTS row from the restored source.
            let _ = super::index::reindex(db);
            return Err(error);
        }
        expired += 1;
    }

    // 2. Staleness sweep: fact/entity/preference past stale_after_days → stale
    let (marked_stale, stale_audit) = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, last_confirmed_at, updated_at, created_at, stale_after_days FROM memories
             WHERE mem_type IN ('fact', 'entity', 'preference')
               AND status = 'active'
               AND stale_after_days IS NOT NULL",
        )?;

        let rows: Vec<(String, Option<String>, String, String, i64)> = stmt
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut marked = 0i64;
        let mut audit_events = Vec::new();
        for (id, last_confirmed, updated_at, created_at, stale_days) in &rows {
            let reference_date = last_confirmed
                .as_deref()
                .or_else(|| (!updated_at.is_empty()).then_some(updated_at.as_str()))
                .unwrap_or(created_at);

            if let Some(ref_dt) = parse_date_flexible(reference_date) {
                let threshold = ref_dt + chrono::Duration::days(*stale_days);
                let today = chrono::Utc::now().date_naive();
                if today > threshold {
                    conn.execute(
                        "UPDATE memories SET status = 'stale' WHERE id = ?1 AND status = 'active'",
                        params![id],
                    )?;
                    audit_events.push((
                        id.clone(),
                        serde_json::json!({"id": id, "staleAfterDays": stale_days}),
                    ));
                    marked += 1;
                }
            }
        }
        Ok::<_, crate::error::AppError>((marked, audit_events))
    })?;

    for (id, detail) in stale_audit {
        if let Err(error) = crate::audit::append_row(
            db,
            "memory-maintenance",
            &id,
            "memory_lifecycle",
            "Memory marked stale",
            &detail,
            None,
            None,
        ) {
            let _ = db.with_conn(|conn| {
                conn.execute(
                    "UPDATE memories SET status = 'active' WHERE id = ?1 AND status = 'stale'",
                    params![id],
                )?;
                Ok(())
            });
            return Err(error);
        }
    }

    Ok(MaintenanceResult {
        expired,
        marked_stale,
    })
}
