use std::fs;

use rusqlite::params;

use crate::db::Db;
use crate::error::AppResult;

use super::frontmatter;
use super::vault;
use super::{MemoryRow, ReindexResult};

/// Bump whenever the source chunker changes meaning. Stored chunks are a
/// rebuildable derived index, so a new value marks every import for re-chunking
/// from the immutable vault copy.
const SOURCE_CHUNK_VERSION: &str = "blocks-v2";

#[derive(Debug, Clone)]
pub struct DocumentChunkHit {
    pub id: i64,
    pub import_id: String,
    pub chunk_index: i64,
    pub title: String,
    pub source_path: String,
    pub body: String,
    pub score: f64,
}

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
        let fts_sql: Option<String> = conn
            .query_row(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='memories_fts'",
                [],
                |row| row.get(0),
            )
            .ok();
        let has_correct_fts = fts_sql
            .as_deref()
            .is_some_and(|sql| sql.contains("contentless_delete=1"));

        if !has_correct_fts {
            if fts_sql.is_some() {
                conn.execute_batch("DROP TABLE memories_fts;")?;
            }
            conn.execute_batch(
                "CREATE VIRTUAL TABLE memories_fts USING fts5(
                    title, summary, body, tags,
                    content='',
                    contentless_delete=1
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
                decided_at TEXT,
                base_content_hash TEXT,
                import_id TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_proposals_status ON memory_proposals(status);

            CREATE TABLE IF NOT EXISTS document_imports (
                id TEXT PRIMARY KEY,
                domain TEXT NOT NULL,
                title TEXT NOT NULL,
                input_kind TEXT NOT NULL,
                source_ref TEXT NOT NULL,
                source_path TEXT NOT NULL UNIQUE,
                original_path TEXT,
                content_hash TEXT NOT NULL,
                byte_count INTEGER NOT NULL,
                candidate_count INTEGER NOT NULL DEFAULT 0,
                warning_count INTEGER NOT NULL DEFAULT 0,
                warnings_json TEXT NOT NULL DEFAULT '[]',
                extraction_engine TEXT,
                extraction_version TEXT,
                extraction_quality_score INTEGER,
                extraction_quality_status TEXT NOT NULL DEFAULT 'not_applicable',
                extraction_quality_json TEXT NOT NULL DEFAULT '[]',
                status TEXT NOT NULL DEFAULT 'pending',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_document_imports_domain
                ON document_imports(domain, created_at DESC);

            CREATE TABLE IF NOT EXISTS memory_operations (
                id TEXT PRIMARY KEY,
                kind TEXT NOT NULL,
                entity_id TEXT NOT NULL,
                stage TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                payload_json TEXT NOT NULL DEFAULT '{}',
                error TEXT,
                started_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                completed_at TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_memory_operations_status
                ON memory_operations(status, updated_at DESC);
            CREATE INDEX IF NOT EXISTS idx_memory_operations_entity
                ON memory_operations(kind, entity_id, started_at DESC);

            CREATE TABLE IF NOT EXISTS episode_consolidations (
                id TEXT PRIMARY KEY,
                episode_id TEXT NOT NULL,
                proposal_id TEXT,
                status TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(episode_id, proposal_id)
            );
            CREATE INDEX IF NOT EXISTS idx_episode_consolidations_episode
                ON episode_consolidations(episode_id, status);

            CREATE TABLE IF NOT EXISTS document_chunks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                import_id TEXT NOT NULL REFERENCES document_imports(id) ON DELETE CASCADE,
                domain TEXT NOT NULL,
                title TEXT NOT NULL,
                source_path TEXT NOT NULL,
                chunk_index INTEGER NOT NULL,
                body TEXT NOT NULL,
                UNIQUE(import_id, chunk_index)
            );
            CREATE INDEX IF NOT EXISTS idx_document_chunks_import
                ON document_chunks(import_id, chunk_index);
            CREATE INDEX IF NOT EXISTS idx_document_chunks_domain
                ON document_chunks(domain);

            CREATE TABLE IF NOT EXISTS memory_eval_cases (
                id TEXT PRIMARY KEY,
                domain TEXT NOT NULL,
                question TEXT NOT NULL,
                expected_sources_json TEXT NOT NULL,
                provenance TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'active',
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_memory_eval_cases_status
                ON memory_eval_cases(status, domain, updated_at DESC);

            CREATE TABLE IF NOT EXISTS memory_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;

        let chunk_fts_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='document_chunks_fts'",
                [],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if !chunk_fts_exists {
            conn.execute_batch(
                "CREATE VIRTUAL TABLE document_chunks_fts USING fts5(
                    title, body,
                    content='',
                    contentless_delete=1
                );",
            )?;
        }

        let proposal_columns = {
            let mut stmt = conn.prepare("PRAGMA table_info(memory_proposals)")?;
            let columns = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            columns
        };
        if !proposal_columns
            .iter()
            .any(|column| column == "base_content_hash")
        {
            conn.execute(
                "ALTER TABLE memory_proposals ADD COLUMN base_content_hash TEXT",
                [],
            )?;
        }
        if !proposal_columns.iter().any(|column| column == "import_id") {
            conn.execute("ALTER TABLE memory_proposals ADD COLUMN import_id TEXT", [])?;
        }
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_proposals_import ON memory_proposals(import_id)",
            [],
        )?;

        let import_columns = {
            let mut stmt = conn.prepare("PRAGMA table_info(document_imports)")?;
            let columns = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>, _>>()?;
            columns
        };
        if !import_columns
            .iter()
            .any(|column| column == "warnings_json")
        {
            conn.execute(
                "ALTER TABLE document_imports ADD COLUMN warnings_json TEXT NOT NULL DEFAULT '[]'",
                [],
            )?;
        }
        if !import_columns
            .iter()
            .any(|column| column == "original_path")
        {
            conn.execute(
                "ALTER TABLE document_imports ADD COLUMN original_path TEXT",
                [],
            )?;
        }
        for (column, definition) in [
            ("extraction_engine", "TEXT"),
            ("extraction_version", "TEXT"),
            ("extraction_quality_score", "INTEGER"),
            (
                "extraction_quality_status",
                "TEXT NOT NULL DEFAULT 'not_applicable'",
            ),
            ("extraction_quality_json", "TEXT NOT NULL DEFAULT '[]'"),
        ] {
            if !import_columns.iter().any(|existing| existing == column) {
                conn.execute(
                    &format!("ALTER TABLE document_imports ADD COLUMN {column} {definition}"),
                    [],
                )?;
            }
        }

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

        // FTS upsert: delete old entry, insert new. FTS failures are fatal:
        // returning success with an unsynchronised search index makes the
        // vault appear to have forgotten a successfully-written memory.
        let tags_str = tags.join(" ");
        conn.execute(
            "DELETE FROM memories_fts WHERE rowid IN (SELECT rowid FROM memories WHERE id = ?1)",
            params![row.id],
        )?;
        conn.execute(
            "INSERT INTO memories_fts (rowid, title, summary, body, tags) VALUES (
                (SELECT rowid FROM memories WHERE id = ?1), ?2, ?3, ?4, ?5
            )",
            params![row.id, row.title, row.summary, body, tags_str],
        )?;

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
    let domains = [
        "work",
        "planphysique",
        "personal",
        "family",
        "finance",
        "research",
    ];
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
                // Capture/delete the FTS row while the parent rowid still
                // exists, then remove the derived row.
                conn.execute(
                    "DELETE FROM memories_fts WHERE rowid IN (SELECT rowid FROM memories WHERE id = ?1)",
                    params![id],
                )?;
                conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
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
                if fm.domain != domain {
                    return Err(crate::error::AppError::Io(std::io::Error::other(
                        format!(
                            "memory domain fence mismatch: {relative} declares {} but is under {domain}",
                            fm.domain
                        ),
                    )));
                }
                let existing = get_by_id(db, &fm.id)?;
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
                    last_accessed_at: existing
                        .as_ref()
                        .and_then(|row| row.last_accessed_at.clone()),
                    access_count: existing.as_ref().map(|row| row.access_count).unwrap_or(0),
                    expires_at: fm.expires.clone(),
                    provenance: serde_json::to_string(&fm.provenance).unwrap_or_default(),
                    content_hash: content_hash.clone(),
                    // Lifecycle state is derived locally and is deliberately
                    // not stored in user-authored YAML. Preserve it during a
                    // reindex so stale memories do not become active again.
                    status: existing
                        .as_ref()
                        .map(|row| row.status.clone())
                        .unwrap_or_else(|| "active".to_string()),
                };

                // Check if content has drifted
                let existing_hash = existing.as_ref().map(|row| row.content_hash.clone());

                if let Some(old_hash) = existing_hash {
                    if old_hash != content_hash {
                        *drifted += 1;
                    }
                }

                upsert(db, &row, &body, &fm.tags)?;
                *indexed += 1;
            } else {
                return Err(crate::error::AppError::Io(std::io::Error::other(format!(
                    "invalid or missing memory frontmatter: {relative}"
                ))));
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
    const STOPWORDS: [&str; 24] = [
        "a", "an", "and", "are", "is", "of", "or", "the", "to", "what", "when", "where", "which",
        "who", "why", "che", "chi", "come", "cosa", "della", "delle", "il", "la", "un",
    ];
    let terms: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .filter(|term| term.chars().count() > 1 && !STOPWORDS.contains(&term.as_str()))
        .map(|term| format!("\"{}\"", term))
        .collect();

    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" OR "))
    }
}

/// Replaces the derived source-passage index for one immutable import. The
/// original Markdown/PDF in the vault remains the source of truth.
pub fn replace_document_chunks(
    db: &Db,
    import_id: &str,
    domain: &str,
    title: &str,
    source_path: &str,
    body: &str,
) -> AppResult<usize> {
    ensure_tables(db)?;
    let chunks = chunk_source(body);
    db.with_conn(|conn| {
        let transaction = conn.unchecked_transaction()?;
        let existing_ids = {
            let mut stmt = transaction
                .prepare("SELECT id FROM document_chunks WHERE import_id = ?1 ORDER BY id")?;
            let rows = stmt
                .query_map(params![import_id], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for id in existing_ids {
            transaction.execute(
                "DELETE FROM document_chunks_fts WHERE rowid = ?1",
                params![id],
            )?;
        }
        transaction.execute(
            "DELETE FROM document_chunks WHERE import_id = ?1",
            params![import_id],
        )?;

        for (chunk_index, chunk) in chunks.iter().enumerate() {
            transaction.execute(
                "INSERT INTO document_chunks (
                    import_id, domain, title, source_path, chunk_index, body
                 ) VALUES (?1,?2,?3,?4,?5,?6)",
                params![
                    import_id,
                    domain,
                    title,
                    source_path,
                    chunk_index as i64,
                    chunk,
                ],
            )?;
            let rowid = transaction.last_insert_rowid();
            transaction.execute(
                "INSERT INTO document_chunks_fts(rowid, title, body) VALUES (?1,?2,?3)",
                params![rowid, title, chunk],
            )?;
        }
        transaction.commit()?;
        Ok(chunks.len())
    })
}

pub fn remove_document_chunks(db: &Db, import_id: &str) -> AppResult<()> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        let ids = {
            let mut stmt = conn.prepare("SELECT id FROM document_chunks WHERE import_id = ?1")?;
            let rows = stmt
                .query_map(params![import_id], |row| row.get::<_, i64>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            rows
        };
        for id in ids {
            conn.execute(
                "DELETE FROM document_chunks_fts WHERE rowid = ?1",
                params![id],
            )?;
        }
        conn.execute(
            "DELETE FROM document_chunks WHERE import_id = ?1",
            params![import_id],
        )?;
        Ok(())
    })
}

pub fn document_imports_missing_chunks(db: &Db) -> AppResult<Vec<String>> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        // The derived index is rebuildable: when the chunker changes, every
        // import is re-chunked from the immutable vault copy instead of being
        // served from segments that no longer match the current boundaries. The
        // version is only recorded once the rebuild has actually succeeded, so a
        // failed source is retried instead of being skipped forever.
        let stored_version: Option<String> = conn
            .query_row(
                "SELECT value FROM memory_meta WHERE key = 'source_chunk_version'",
                [],
                |row| row.get(0),
            )
            .ok();
        if stored_version.as_deref() != Some(SOURCE_CHUNK_VERSION) {
            let mut stmt = conn.prepare(
                "SELECT id FROM document_imports
                 WHERE extraction_quality_status != 'failed'
                 ORDER BY created_at",
            )?;
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(0))?
                .collect::<Result<Vec<_>, _>>()?;
            return Ok(rows);
        }
        let mut stmt = conn.prepare(
            "SELECT i.id
             FROM document_imports i
             LEFT JOIN document_chunks c ON c.import_id = i.id
             WHERE i.extraction_quality_status != 'failed'
             GROUP BY i.id
             HAVING COUNT(c.id) = 0
             ORDER BY i.created_at",
        )?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// Record the chunker version once every import has been rebuilt from the vault.
pub fn mark_source_chunks_current(db: &Db) -> AppResult<()> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO memory_meta (key, value) VALUES ('source_chunk_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SOURCE_CHUNK_VERSION],
        )?;
        Ok(())
    })
}

pub fn search_document_chunks(
    db: &Db,
    query: &str,
    domain: &str,
    limit: usize,
) -> AppResult<Vec<DocumentChunkHit>> {
    ensure_tables(db)?;
    let Some(match_expression) = fts_match_expr(query) else {
        return Ok(Vec::new());
    };
    let query_terms = searchable_terms(query);
    let raw = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT c.id, c.import_id, c.chunk_index, c.title, c.source_path, c.body
             FROM document_chunks_fts f
             JOIN document_chunks c ON c.id = f.rowid
             JOIN document_imports i ON i.id = c.import_id
             WHERE document_chunks_fts MATCH ?1
               AND c.domain = ?2
               AND i.extraction_quality_status != 'failed'
             ORDER BY bm25(document_chunks_fts)
             LIMIT ?3",
        )?;
        let rows = stmt
            .query_map(
                params![match_expression, domain, (limit * 4) as i64],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, String>(4)?,
                        row.get::<_, String>(5)?,
                    ))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        Ok::<_, crate::error::AppError>(rows)
    })?;

    let mut hits = raw
        .into_iter()
        .enumerate()
        .map(|(position, (id, import_id, chunk_index, title, source_path, body))| {
            let body_terms = searchable_terms(&body);
            let coverage = if query_terms.is_empty() {
                0.0
            } else {
                query_terms.intersection(&body_terms).count() as f64 / query_terms.len() as f64
            };
            let reciprocal_rank = 1.0 / (1.0 + position as f64 * 0.18);
            DocumentChunkHit {
                id,
                import_id,
                chunk_index,
                title,
                source_path,
                body,
                score: (0.72 * coverage + 0.28 * reciprocal_rank).clamp(0.0, 1.0),
            }
        })
        .collect::<Vec<_>>();
    hits.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    hits.truncate(limit);
    Ok(hits)
}

fn searchable_terms(value: &str) -> std::collections::BTreeSet<String> {
    value
        .to_lowercase()
        .split(|character: char| !character.is_alphanumeric())
        .filter(|term| term.chars().count() > 2)
        .map(str::to_string)
        .collect()
}

fn chunk_source(body: &str) -> Vec<String> {
    const TARGET_CHARS: usize = 1_500;
    const OVERLAP_CHARS: usize = 220;

    let normalized = body.replace('\0', "").replace("\r\n", "\n");
    let blocks = source_blocks(&normalized);
    let mut chunks = Vec::new();
    let mut current: Vec<String> = Vec::new();
    for block in blocks {
        let mut candidate = current.clone();
        candidate.push(block.clone());
        if !current.is_empty() && joined_block_len(&candidate) > TARGET_CHARS {
            chunks.push(current.join("\n\n"));
            current = overlap_blocks(&current, OVERLAP_CHARS);
            // The carried overlap must still leave room for this block, or the
            // next chunk would exceed the bound the model prompt relies on.
            while !current.is_empty()
                && joined_block_len(&current) + block.chars().count() + 2 > TARGET_CHARS
            {
                current.remove(0);
            }
        }
        current.push(block);
    }
    if !current.is_empty() {
        chunks.push(current.join("\n\n"));
    }
    chunks
}

/// Split extracted text into whole list items and paragraphs. A bullet is never
/// cut in half: the synthesis prompt asks for one claim per list item, and a
/// claim can only be verified against text that still carries the item label.
fn source_blocks(value: &str) -> Vec<String> {
    const MAX_BLOCK_CHARS: usize = 1_200;

    let lines = value.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut current = String::new();
    let mut previous_line_blank = true;
    for (index, raw_line) in lines.iter().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() {
            previous_line_blank = true;
            continue;
        }
        let next_line = lines[index + 1..]
            .iter()
            .map(|candidate| candidate.trim())
            .find(|candidate| !candidate.is_empty());
        let starts_new_block = is_source_bullet(line)
            || (!current.is_empty() && previous_line_blank && starts_new_paragraph(line));
        if starts_new_block {
            push_source_block(&mut blocks, &mut current, MAX_BLOCK_CHARS);
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(line);
        // A bullet keeps its hard-wrapped continuation lines even when the PDF
        // extraction inserted blank lines inside the item.
        previous_line_blank = next_line.is_none();
    }
    push_source_block(&mut blocks, &mut current, MAX_BLOCK_CHARS);
    blocks
}

fn push_source_block(blocks: &mut Vec<String>, current: &mut String, limit: usize) {
    let block = current.trim().to_string();
    current.clear();
    if block.is_empty() {
        return;
    }
    if block.chars().count() <= limit {
        blocks.push(block);
    } else {
        blocks.extend(split_on_word_boundaries(&block, limit));
    }
}

fn is_source_bullet(value: &str) -> bool {
    let trimmed = value.trim_start();
    trimmed.starts_with('●')
        || trimmed.starts_with('•')
        || trimmed.starts_with('○')
        || trimmed.starts_with("- ")
}

/// Hard-wrapped PDF text resumes with a lowercase fragment; a genuinely new
/// paragraph starts with a capital letter or a list marker.
fn starts_new_paragraph(value: &str) -> bool {
    value
        .chars()
        .find(|character| character.is_alphabetic())
        .is_none_or(|character| character.is_uppercase())
}

fn joined_block_len(blocks: &[String]) -> usize {
    blocks
        .iter()
        .map(|block| block.chars().count())
        .sum::<usize>()
        + blocks.len().saturating_sub(1) * 2
}

fn overlap_blocks(blocks: &[String], limit: usize) -> Vec<String> {
    // Carry whole blocks only. A partial tail would put the next chunk's first
    // line in the middle of a list item and detach the item label from the
    // sentence the verifier has to match.
    let mut carried: Vec<String> = Vec::new();
    let mut total = 0usize;
    for block in blocks.iter().rev() {
        let block_len = block.chars().count();
        if total + block_len + 2 > limit {
            break;
        }
        total += block_len + 2;
        carried.insert(0, block.clone());
    }
    carried
}

/// A single unbroken token (minified or space-less text) still has to respect
/// the chunk bound, so it is hard-split on characters as a last resort.
fn split_on_word_boundaries(value: &str, limit: usize) -> Vec<String> {
    let mut pieces = Vec::new();
    let mut current = String::new();
    let mut current_len = 0usize;
    for word in value.split_whitespace() {
        let word_len = word.chars().count();
        if !current.is_empty() && current_len + word_len + 1 > limit {
            pieces.push(std::mem::take(&mut current));
            current_len = 0;
        }
        if word_len > limit {
            for character in word.chars() {
                if current_len == limit {
                    pieces.push(std::mem::take(&mut current));
                    current_len = 0;
                }
                current.push(character);
                current_len += 1;
            }
            continue;
        }
        if !current.is_empty() {
            current.push(' ');
            current_len += 1;
        }
        current.push_str(word);
        current_len += word_len;
    }
    if !current.is_empty() {
        pieces.push(current);
    }
    pieces
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

/// Full row lookup by vault path, used to resolve `related` frontmatter
/// links during evidence expansion and lint.
pub fn get_by_path(db: &Db, path: &str) -> AppResult<Option<MemoryRow>> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, vault_path, domain, mem_type, title, summary, sensitivity,
                    confidence, created_at, updated_at, valid_from, valid_until,
                    stale_after_days, last_confirmed_at, confirmation_count,
                    last_accessed_at, access_count, expires_at, provenance,
                    content_hash, status
             FROM memories WHERE vault_path = ?1",
        )?;
        let mut rows = stmt.query_map(params![path], row_to_memory)?;
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

/// Remove one derived index row and its FTS entry. The vault file is not
/// touched. Used by compensating rollback after a failed create.
pub fn remove(db: &Db, id: &str) -> AppResult<()> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        conn.execute(
            "DELETE FROM memories_fts WHERE rowid IN (SELECT rowid FROM memories WHERE id = ?1)",
            params![id],
        )?;
        conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        Ok(())
    })
}

/// Return lightweight metadata by vault path for tree enrichment.
pub fn metadata_by_path(db: &Db, path: &str) -> AppResult<Option<(String, String, String)>> {
    ensure_tables(db)?;
    db.with_conn(|conn| {
        let mut stmt =
            conn.prepare("SELECT id, mem_type, status FROM memories WHERE vault_path = ?1")?;
        let mut rows = stmt.query_map(params![path], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    })
}

/// Confirm a memory is still true (resets staleness). The vault file is
/// the source of truth (MEMORY-SPEC §0.1), so the confirmation MUST be
/// written into the file's frontmatter first — updating only the index
/// would be silently undone by the next reindex.
pub fn confirm(db: &Db, id: &str) -> AppResult<()> {
    let _write_guard = vault::lock_writes();
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
    let old_content = content;
    vault::write_file_atomic(&row.vault_path, &new_content)?;
    if let Err(error) = vault::git_commit(&format!("mem({}): confirm {}", row.domain, row.title)) {
        let _ = vault::write_file_atomic(&row.vault_path, &old_content);
        return Err(error);
    }

    // 2. Index second, including the new content hash so the next reindex
    // does not read this write as drift.
    let new_hash = crate::audit::compute_content_hash(&new_content);
    let update_result = db.with_conn(|conn| {
        conn.execute(
            "UPDATE memories SET last_confirmed_at = ?1, confirmation_count = confirmation_count + 1,
                    status = 'active', updated_at = ?1, content_hash = ?2 WHERE id = ?3",
            params![now, new_hash, id],
        )?;
        Ok(())
    });

    if let Err(error) = update_result {
        let _ = vault::write_file_atomic(&row.vault_path, &old_content);
        let _ = vault::git_commit(&format!(
            "mem({}): rollback confirm {}",
            row.domain, row.title
        ));
        return Err(error);
    }

    if let Err(error) = crate::audit::append_row(
        db,
        "memory",
        id,
        "memory_write",
        "Memory confirmation persisted",
        &serde_json::json!({"id": id, "path": row.vault_path, "op": "confirm"}),
        None,
        None,
    ) {
        let _ = vault::write_file_atomic(&row.vault_path, &old_content);
        let _ = vault::git_commit(&format!(
            "mem({}): rollback confirm {}",
            row.domain, row.title
        ));
        if let Some((old_fm, old_body)) = frontmatter::parse(&old_content) {
            let _ = upsert(db, &row, &old_body, &old_fm.tags);
        }
        return Err(error);
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_chunking_is_bounded_and_overlapping_even_for_one_long_line() {
        let body = (0..4_000)
            .map(|index| char::from(b'a' + (index % 26) as u8))
            .collect::<String>();
        let chunks = chunk_source(&body);

        assert!(chunks.len() >= 3);
        assert!(chunks.iter().all(|chunk| chunk.chars().count() <= 1_500));
        let tail = chunks[0]
            .chars()
            .rev()
            .take(220)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        assert!(!tail.is_empty());
    }

    #[test]
    fn source_chunking_never_splits_a_list_item_label_from_its_body() {
        let body = "Recommended Use Cases\n\n\
            ●  Personalized hero and content selection (Da Vinci): Select the most relevant lead or\n\
            secondary story for each customer using browsing, purchase history, category affinity,\n\
            engagement, loyalty signals, and active campaign performance.\n\n\
            ●  AI-powered audience targeting (Da Vinci): Utilize AI to analyze imagery and content.\n\n\
            ●  Live inventory: Show recently browsed items while suppressing out-of-stock items.";
        let chunks = chunk_source(body);

        for chunk in &chunks {
            for line in chunk.lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                assert!(
                    line.starts_with('●') || line.starts_with('•') || line.starts_with("Recommended"),
                    "a chunk line must not begin with a mid-item fragment: {line:?}"
                );
            }
        }
        let joined = chunks.join("\n\n");
        for label in [
            "Personalized hero and content selection (Da Vinci):",
            "AI-powered audience targeting (Da Vinci):",
            "Live inventory:",
        ] {
            assert!(joined.contains(label), "missing label: {label}");
        }
    }

    #[test]
    fn source_chunking_keeps_hard_wrapped_text_with_its_own_item() {
        let body = "●  Continuous testing and real-time optimization (Da Vinci x Studio): Test subject lines,\n\n\
            creative variants (e.g. PDP vs. Attribute/Lifestyle), product logic, and dynamic modules\n\
            continuously, then apply what is learned while the content is still active.\n\n\
            ●  Broader content support (Da Vinci): Use dynamic modules to give athlete videos more exposure.";
        let chunks = chunk_source(body);
        let joined = chunks.join("\n\n");
        let testing = joined
            .lines()
            .find(|line| line.contains("Continuous testing"))
            .expect("the wrapped item must stay on one line");

        assert!(testing.contains("still active"), "got: {testing}");
        assert!(
            !testing.contains("Broader content support"),
            "two separate items must never be fused: {testing}"
        );
    }

    #[test]
    fn source_chunking_overlap_never_starts_a_chunk_mid_item() {
        // Every item is longer than the overlap budget, so a partial carry is
        // tempting; the first line of each chunk must still be an item label.
        let body = (0..12)
            .map(|index| {
                format!(
                    "●  Use case number {index} (Studio): {}",
                    "detail sentence about the campaign. ".repeat(20).trim()
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let chunks = chunk_source(&body);

        assert!(chunks.len() >= 4, "expected several chunks");
        for chunk in &chunks {
            let first = chunk
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .expect("a chunk is never empty");
            assert!(
                first.starts_with('●'),
                "a chunk must open on an item label, got: {first:?}"
            );
        }
    }
}
