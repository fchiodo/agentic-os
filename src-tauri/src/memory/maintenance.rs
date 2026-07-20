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
    super::index::ensure_tables(db)?;
    let now_date = chrono::Utc::now().format("%Y-%m-%d").to_string();

    // 1. TTL sweep: episodes past expires_at → expired, move to archive.
    // substr(...,1,10) normalizes RFC 3339 timestamps to their date part so
    // the lexicographic comparison stays correct for both stored shapes.
    let expired = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, vault_path, domain FROM memories
             WHERE mem_type = 'episode' AND expires_at IS NOT NULL AND substr(expires_at, 1, 10) < ?1 AND status != 'expired'",
        )?;

        let rows: Vec<(String, String, String)> = stmt
            .query_map(params![now_date], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut expired = 0i64;
        for (id, path, domain) in &rows {
            // Move file to archive
            let _ = vault::archive_file(path, domain);

            // Update status
            conn.execute(
                "UPDATE memories SET status = 'expired' WHERE id = ?1",
                params![id],
            )?;

            // Remove from FTS
            let _ = conn.execute(
                "DELETE FROM memories_fts WHERE rowid IN (SELECT rowid FROM memories WHERE id = ?1)",
                params![id],
            );

            expired += 1;
        }
        Ok::<_, crate::error::AppError>(expired)
    })?;

    // 2. Staleness sweep: fact/entity/preference past stale_after_days → stale
    let marked_stale = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, last_confirmed_at, stale_after_days FROM memories
             WHERE mem_type IN ('fact', 'entity', 'preference')
               AND status = 'active'
               AND stale_after_days IS NOT NULL",
        )?;

        let rows: Vec<(String, Option<String>, i64)> = stmt
            .query_map([], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut marked = 0i64;
        for (id, last_confirmed, stale_days) in &rows {
            let reference_date = last_confirmed
                .as_deref()
                .unwrap_or(&now_date);

            if let Some(ref_dt) = parse_date_flexible(reference_date) {
                let threshold = ref_dt + chrono::Duration::days(*stale_days);
                let today = chrono::Utc::now().date_naive();
                if today > threshold {
                    conn.execute(
                        "UPDATE memories SET status = 'stale' WHERE id = ?1 AND status = 'active'",
                        params![id],
                    )?;
                    marked += 1;
                }
            }
        }
        Ok::<_, crate::error::AppError>(marked)
    })?;

    Ok(MaintenanceResult {
        expired,
        marked_stale,
    })
}
