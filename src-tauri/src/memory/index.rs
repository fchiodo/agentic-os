use std::fs;

use rusqlite::params;

use crate::db::Db;
use crate::error::AppResult;

use super::frontmatter;
use super::vault;
use super::{MemoryRow, ReindexResult};

/// Create the memory tables if they don't exist.
pub fn ensure_tables(db: &Db) -> AppResult<()> {
    db.with_conn(|conn| {
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memories (
                id TEXT PRIMARY KEY,
                vault_path TEXT NOT NULL UNIQUE,
                domain TEXT NOT NULL,
                mem_type TEXT NOT NULL,
                title TEXT NOT NULL,
                summary TEXT,
                sensitivity TEXT NOT NULL DEFAULT 'normal',
                confidence REAL NOT NULL DEFAULT 0.7,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                valid_from TEXT,
                valid_until TEXT,
                stale_after_days INTEGER,
                last_confirmed_at TEXT,
                confirmation_count INTEGER NOT NULL DEFAULT 0,
                last_accessed_at TEXT,
                access_count INTEGER NOT NULL DEFAULT 0,
                expires_at TEXT,
                provenance TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active'
            );
            CREATE INDEX IF NOT EXISTS idx_memories_domain ON memories(domain, status);
            CREATE INDEX IF NOT EXISTS idx_memories_status ON memories(status);
            "#,
        )?;

        // FTS5 virtual table — create only if not exists (CREATE VIRTUAL TABLE
        // doesn't support IF NOT EXISTS in older sqlite builds, so we check)
        let has_fts: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='memories_fts'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0)
            > 0;

        if !has_fts {
            conn.execute_batch(
                "CREATE VIRTUAL TABLE memories_fts USING fts5(
                    title, summary, body, tags,
                    content='',
                    contentless_delete
                );",
            )?;
        }

        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS memory_proposals (
                id TEXT PRIMARY KEY,
                task_id TEXT,
                vault_path TEXT NOT NULL,
                domain TEXT NOT NULL,
                kind TEXT NOT NULL,
                op TEXT NOT NULL,
                supersedes_id TEXT,
                sensitivity TEXT NOT NULL,
                unified_diff TEXT NOT NULL,
                new_content TEXT NOT NULL,
                provenance TEXT NOT NULL,
                gate_report TEXT NOT NULL,
                requires_approval INTEGER NOT NULL,
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                decided_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_proposals_status ON memory_proposals(status);
            "#,
        )?;

        Ok(())
    })
}

/// Upsert a memory row + FTS entry.
pub fn upsert(db: &Db, row: &MemoryRow, body: &str, tags: &[String]) -> AppResult<()> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO memories (
                id, vault_path, domain, mem_type, title, summary, sensitivity,
                confidence, created_at, updated_at, valid_from, valid_until,
                stale_after_days, last_confirmed_at, confirmation_count,
                last_accessed_at, access_count, expires_at, provenance,
                content_hash, status
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21)
            ON CONFLICT(id) DO UPDATE SET
                vault_path=excluded.vault_path, domain=excluded.domain,
                mem_type=excluded.mem_type, title=excluded.title,
                summary=excluded.summary, sensitivity=excluded.sensitivity,
                confidence=excluded.confidence, created_at=excluded.created_at,
                updated_at=excluded.updated_at, valid_from=excluded.valid_from,
                valid_until=excluded.valid_until,
                stale_after_days=excluded.stale_after_days,
                last_confirmed_at=excluded.last_confirmed_at,
                confirmation_count=excluded.confirmation_count,
                expires_at=excluded.expires_at,
                provenance=excluded.provenance,
                content_hash=excluded.content_hash,
                status=excluded.status",
            params![
                row.id,
                row.vault_path,
                row.domain,
                row.mem_type,
                row.title,
                row.summary,
                row.sensitivity,
                row.confidence,
                row.created_at,
                row.updated_at,
                row.valid_from,
                row.valid_until,
                row.stale_after_days,
                row.last_confirmed_at,
                row.confirmation_count,
                row.last_accessed_at,
                row.access_count,
                row.expires_at,
                row.provenance,
                row.content_hash,
                row.status,
            ],
        )?;

        // FTS upsert: delete old entry, insert new
        let tags_str = tags.join(" ");
        let _ = conn.execute(
            "DELETE FROM memories_fts WHERE rowid IN (SELECT rowid FROM memories WHERE id = ?1)",
            params![row.id],
        );
        let _ = conn.execute(
            "INSERT INTO memories_fts (rowid, title, summary, body, tags) VALUES (
                (SELECT rowid FROM memories WHERE id = ?1), ?2, ?3, ?4, ?5
            )",
            params![row.id, row.title, row.summary, body, tags_str],
        );

        Ok(())
    })
}

/// Reindex: scan all vault files, detect drift, and orphaned DB rows.
pub fn reindex(db: &Db) -> AppResult<ReindexResult> {
    ensure_tables(db)?;

    let root = vault::vault_root()?;
    let mut indexed = 0i64;
    let mut drifted = 0i64;

    // Walk all domain directories
    let domains = ["work", "planphysique", "personal", "family", "finance", "research"];
    for domain in &domains {
        let dir = root.join(domain);
        if !dir.exists() {
            continue;
        }
        walk_and_index(db, &root, &dir, domain, &mut indexed, &mut drifted)?;
    }

    // Find orphaned DB rows (files that no longer exist)
    let orphaned = db.with_conn(|conn| {
        let mut stmt = conn.prepare("SELECT id, vault_path FROM memories")?;
        let rows: Vec<(String, String)> = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?
            .collect::<Result<Vec<_>, _>>()?;

        let mut orphaned = 0i64;
        for (id, path) in &rows {
            let full = root.join(path);
            if !full.exists() {
                conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
                let _ = conn.execute(
                    "DELETE FROM memories_fts WHERE rowid IN (SELECT rowid FROM memories WHERE id = ?1)",
                    params![id],
                );
                orphaned += 1;
            }
        }
        Ok::<_, crate::error::AppError>(orphaned)
    })?;

    Ok(ReindexResult {
        indexed,
        drifted,
        orphaned,
    })
}

fn walk_and_index(
    db: &Db,
    root: &std::path::Path,
    dir: &std::path::Path,
    domain: &str,
    indexed: &mut i64,
    drifted: &mut i64,
) -> AppResult<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            if name == "_archive" || name.starts_with('.') {
                continue;
            }
            walk_and_index(db, root, &path, domain, indexed, drifted)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let relative = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .to_string();

            let content = fs::read_to_string(&path)?;
            let content_hash = crate::audit::compute_content_hash(&content);

            if let Some((fm, body)) = frontmatter::parse(&content) {
                let row = MemoryRow {
                    id: fm.id.clone(),
                    vault_path: relative.clone(),
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
                    provenance: serde_json::to_string(&fm.provenance)
                        .unwrap_or_default(),
                    content_hash: content_hash.clone(),
                    status: "active".to_string(),
                };

                // Check if content has drifted
                let existing_hash: Option<String> = db.with_conn(|conn| -> Result<Option<String>, crate::error::AppError> {
                    Ok(conn.query_row(
                        "SELECT content_hash FROM memories WHERE id = ?1",
                        params![fm.id],
                        |row| row.get(0),
                    ).ok())
                }).unwrap_or(None);

                if let Some(old_hash) = existing_hash {
                    if old_hash != content_hash {
                        *drifted += 1;
                    }
                }

                upsert(db, &row, &body, &fm.tags)?;
                *indexed += 1;
            }
        }
    }
    Ok(())
}

/// Sanitize arbitrary user text into a safe FTS5 MATCH expression: each
/// alphanumeric term is double-quoted (so apostrophes, hyphens, and FTS
/// operators in the input can never break query syntax) and terms are
/// OR-joined. Returns None when no searchable term survives.
pub fn fts_match_expr(input: &str) -> Option<String> {
    let terms: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(|t| format!("\"{}\"", t))
        .collect();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// Get a memory row by ID.
pub fn get_by_id(db: &Db, id: &str) -> AppResult<Option<MemoryRow>> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, vault_path, domain, mem_type, title, summary, sensitivity,
                    confidence, created_at, updated_at, valid_from, valid_until,
                    stale_after_days, last_confirmed_at, confirmation_count,
                    last_accessed_at, access_count, expires_at, provenance,
                    content_hash, status
             FROM memories WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], row_to_memory)?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    })
}

/// List all memories for a domain. Reserved for the Memory UI browse
/// view (MEMORY-SPEC §9) which lists by domain without a search query.
#[allow(dead_code)]
pub fn list_by_domain(db: &Db, domain: &str) -> AppResult<Vec<MemoryRow>> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, vault_path, domain, mem_type, title, summary, sensitivity,
                    confidence, created_at, updated_at, valid_from, valid_until,
                    stale_after_days, last_confirmed_at, confirmation_count,
                    last_accessed_at, access_count, expires_at, provenance,
                    content_hash, status
             FROM memories WHERE domain = ?1 AND status != 'expired'
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map(params![domain], row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// List all active memories. Reserved for the Memory UI browse view.
#[allow(dead_code)]
pub fn list_all(db: &Db) -> AppResult<Vec<MemoryRow>> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, vault_path, domain, mem_type, title, summary, sensitivity,
                    confidence, created_at, updated_at, valid_from, valid_until,
                    stale_after_days, last_confirmed_at, confirmation_count,
                    last_accessed_at, access_count, expires_at, provenance,
                    content_hash, status
             FROM memories WHERE status != 'expired'
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt
            .query_map([], row_to_memory)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Update access stats.
pub fn touch(db: &Db, id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE memories SET last_accessed_at = ?1, access_count = access_count + 1 WHERE id = ?2",
            params![now, id],
        )?;
        Ok(())
    })
}

/// Confirm a memory is still true (resets staleness). The vault file is
/// the source of truth (MEMORY-SPEC §0.1), so the confirmation MUST be
/// written into the file's frontmatter first — updating only the index
/// would be silently undone by the next reindex.
pub fn confirm(db: &Db, id: &str) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    let row = get_by_id(db, id)?
        .ok_or_else(|| crate::error::AppError::Io(std::io::Error::other("memory not found")))?;

    // 1. File first: bump last_confirmed / confirmations in frontmatter.
    let (content, _) = vault::read_file(&row.vault_path)?;
    let (mut fm, body) = frontmatter::parse(&content).ok_or_else(|| {
        crate::error::AppError::Io(std::io::Error::other(
            "memory file has no parseable frontmatter",
        ))
    })?;
    fm.last_confirmed = Some(now.clone());
    fm.confirmations = Some(fm.confirmations.unwrap_or(0) + 1);
    fm.updated = now.clone();
    let new_content = frontmatter::serialize(&fm, &body);
    vault::write_file(&row.vault_path, &new_content)?;
    let _ = vault::git_commit(&format!("mem({}): confirm {}", row.domain, row.title));

    // 2. Index second, including the new content hash so the next reindex
    // does not read this write as drift.
    let new_hash = crate::audit::compute_content_hash(&new_content);
    db.with_conn(|conn| {
        conn.execute(
            "UPDATE memories SET last_confirmed_at = ?1, confirmation_count = confirmation_count + 1,
                    status = 'active', updated_at = ?1, content_hash = ?2 WHERE id = ?3",
            params![now, new_hash, id],
        )?;
        Ok(())
    })
}

fn row_to_memory(row: &rusqlite::Row) -> rusqlite::Result<MemoryRow> {
    Ok(MemoryRow {
        id: row.get(0)?,
        vault_path: row.get(1)?,
        domain: row.get(2)?,
        mem_type: row.get(3)?,
        title: row.get(4)?,
        summary: row.get(5)?,
        sensitivity: row.get(6)?,
        confidence: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
        valid_from: row.get(10)?,
        valid_until: row.get(11)?,
        stale_after_days: row.get(12)?,
        last_confirmed_at: row.get(13)?,
        confirmation_count: row.get(14)?,
        last_accessed_at: row.get(15)?,
        access_count: row.get(16)?,
        expires_at: row.get(17)?,
        provenance: row.get(18)?,
        content_hash: row.get(19)?,
        status: row.get(20)?,
    })
}
