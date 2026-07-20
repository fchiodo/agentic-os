use rusqlite::params;

use crate::db::Db;
use crate::error::AppResult;

use super::{MemorySearchOpts, ScoredMemory};

/// Half-lives for recency decay per type (in days).
fn half_life_days(mem_type: &str) -> f64 {
    match mem_type {
        "episode" => 30.0,
        "fact" => 180.0,
        "decision" => 730.0,
        "preference" | "entity" => 365.0,
        _ => 180.0,
    }
}

/// Search memories using FTS BM25 + scoring formula from the spec.
pub fn search(
    db: &Db,
    query: &str,
    domain: Option<&str>,
    opts: &MemorySearchOpts,
) -> AppResult<Vec<ScoredMemory>> {
    super::index::ensure_tables(db)?;

    let limit = opts.limit.unwrap_or(8) as i64;

    // 1. FTS search
    let fts_results = search_fts(db, query, limit * 5)?; // over-fetch for ranking

    // 2. Score each candidate
    let mut scored: Vec<ScoredMemory> = Vec::new();
    for (id, bm25_score) in fts_results {
        if let Some(row) = super::index::get_by_id(db, &id)? {
            // Permission filter
            if let Some(d) = domain {
                if row.domain != d {
                    continue;
                }
            }

            // Stale filter
            if row.status == "stale" && !opts.include_stale {
                continue;
            }
            if row.status == "expired" {
                continue;
            }

            let scored_memory = score_row(&row, bm25_score);
            scored.push(scored_memory);
        }
    }

    // 3. Sort by composite score descending
    scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));

    // 4. Take top K and update access stats
    let result: Vec<ScoredMemory> = scored.into_iter().take(limit as usize).collect();
    for m in &result {
        let _ = super::index::touch(db, &m.row.id);
    }

    Ok(result)
}

/// BM25 search against the FTS table. The FTS rowid mirrors the rowid of
/// the `memories` table (see index::upsert), so the join must go through
/// m.rowid — joining on the TEXT uuid would never match.
fn search_fts(db: &Db, query: &str, limit: i64) -> AppResult<Vec<(String, f64)>> {
    let Some(match_expr) = super::index::fts_match_expr(query) else {
        return Ok(Vec::new());
    };

    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT m.id, bm25(memories_fts) as rank
             FROM memories_fts f
             JOIN memories m ON m.rowid = f.rowid
             WHERE memories_fts MATCH ?1
             ORDER BY rank
             LIMIT ?2",
        )?;

        let rows = stmt
            .query_map(params![match_expr, limit], |row| {
                let id: String = row.get(0)?;
                let rank: f64 = row.get(1)?;
                // bm25 returns negative values (lower = more relevant)
                // Normalize: convert to [0, 1] range
                let normalized = 1.0 / (1.0 + rank.abs());
                Ok((id, normalized))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    })
}

fn score_row(row: &super::MemoryRow, bm25_normalized: f64) -> ScoredMemory {
    // Recency: exponential decay
    let age_days = parse_age_days(&row.updated_at);
    let hl = half_life_days(&row.mem_type);
    let recency = (-0.693147_f64 * age_days / hl).exp(); // ln(2) ≈ 0.693147

    // Trust: confidence * min(1, 0.6 + 0.1 * confirmation_count)
    let trust = row.confidence * (0.6 + 0.1 * row.confirmation_count as f64).min(1.0);

    // Composite score
    let mut score = 0.60 * bm25_normalized + 0.25 * recency + 0.15 * trust;
    if row.status == "stale" {
        score -= 0.30;
    }
    score = score.max(0.0);

    ScoredMemory {
        row: row.clone(),
        score,
        relevance: bm25_normalized,
        recency,
        trust,
    }
}

fn parse_age_days(date_str: &str) -> f64 {
    // Try parsing as ISO 8601 / RFC 3339
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(date_str) {
        let now = chrono::Utc::now();
        let duration = now.signed_duration_since(dt);
        return duration.num_days() as f64;
    }
    // Fallback: try parsing as date only
    if let Ok(dt) = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
        let today = chrono::Utc::now().date_naive();
        return (today - dt).num_days() as f64;
    }
    0.0
}
