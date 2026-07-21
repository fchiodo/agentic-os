use rusqlite::params;

use crate::db::Db;
use crate::error::AppResult;

use super::{MemoryAnswer, MemoryAskRequest, MemoryCitation, MemorySearchOpts, ScoredMemory};

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

    if domain.is_some_and(|value| {
        !matches!(
            value,
            "work" | "planphysique" | "personal" | "family" | "finance" | "research"
        )
    }) {
        return Err(crate::error::AppError::Io(std::io::Error::other(
            "invalid memory domain",
        )));
    }
    if query.chars().count() > 1_000 {
        return Err(crate::error::AppError::Io(std::io::Error::other(
            "memory query is too long",
        )));
    }
    let limit = opts.limit.unwrap_or(8).clamp(1, 50) as i64;

    if query.trim().is_empty() {
        return Ok(Vec::new());
    }

    // Permission and lifecycle filters happen inside SQL, before candidates
    // leave storage. Exact-title matches form a separate high-confidence lane.
    let mut candidates = search_exact(db, query, domain, opts.include_stale, limit)?;
    let mut seen: std::collections::HashSet<String> =
        candidates.iter().map(|(id, _)| id.clone()).collect();
    for candidate in search_fts(db, query, domain, opts.include_stale, limit * 5)? {
        if seen.insert(candidate.0.clone()) {
            candidates.push(candidate);
        }
    }

    // 2. Score each candidate
    let mut scored: Vec<ScoredMemory> = Vec::new();
    for (id, bm25_score) in candidates {
        if let Some(row) = super::index::get_by_id(db, &id)? {
            let scored_memory = score_row(&row, bm25_score);
            scored.push(scored_memory);
        }
    }

    // 3. Sort by composite score descending
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // 4. Take top K and update access stats
    let result: Vec<ScoredMemory> = scored.into_iter().take(limit as usize).collect();
    for m in &result {
        super::index::touch(db, &m.row.id)?;
    }

    Ok(result)
}

/// BM25 search against the FTS table. The FTS rowid mirrors the rowid of
/// the `memories` table (see index::upsert), so the join must go through
/// m.rowid — joining on the TEXT uuid would never match.
fn search_exact(
    db: &Db,
    query: &str,
    domain: Option<&str>,
    include_stale: bool,
    limit: i64,
) -> AppResult<Vec<(String, f64)>> {
    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id FROM memories
             WHERE lower(trim(title)) = lower(trim(?1))
               AND (?2 IS NULL OR domain = ?2)
               AND status != 'expired'
               AND (?3 = 1 OR status != 'stale')
             ORDER BY updated_at DESC LIMIT ?4",
        )?;
        let rows = stmt
            .query_map(params![query, domain, include_stale as i64, limit], |row| {
                Ok((row.get(0)?, 1.0))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

fn search_fts(
    db: &Db,
    query: &str,
    domain: Option<&str>,
    include_stale: bool,
    limit: i64,
) -> AppResult<Vec<(String, f64)>> {
    let Some(match_expr) = super::index::fts_match_expr(query) else {
        return Ok(Vec::new());
    };

    db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT m.id, bm25(memories_fts) as rank
             FROM memories_fts f
             JOIN memories m ON m.rowid = f.rowid
             WHERE memories_fts MATCH ?1
               AND (?2 IS NULL OR m.domain = ?2)
               AND m.status != 'expired'
               AND (?3 = 1 OR m.status != 'stale')
             ORDER BY rank
             LIMIT ?4",
        )?;

        let raw = stmt
            .query_map(
                params![match_expr, domain, include_stale as i64, limit],
                |row| {
                    let id: String = row.get(0)?;
                    let rank: f64 = row.get(1)?;
                    Ok((id, rank.abs()))
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;
        let max_rank = raw.iter().map(|(_, rank)| *rank).fold(0.0_f64, f64::max);
        Ok(raw
            .into_iter()
            .enumerate()
            .map(|(position, (id, rank))| {
                let relative = if max_rank > 0.0 { rank / max_rank } else { 0.0 };
                let reciprocal_rank = 1.0 / (1.0 + position as f64 * 0.12);
                (
                    id,
                    (0.65 * relative + 0.35 * reciprocal_rank).clamp(0.0, 1.0),
                )
            })
            .collect())
    })
}

fn query_terms(value: &str) -> std::collections::BTreeSet<String> {
    value
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| term.chars().count() > 2)
        .map(str::to_string)
        .collect()
}

fn best_excerpt(body: &str, question: &str) -> String {
    let terms = query_terms(question);
    body.split(['\n', '.', '!', '?'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .max_by_key(|part| {
            let part_terms = query_terms(part);
            part_terms.intersection(&terms).count()
        })
        .unwrap_or(body.trim())
        .chars()
        .take(420)
        .collect()
}

/// Deterministic, grounded Q&A over the vault. This intentionally produces
/// an extractive answer: every statement is copied from a cited memory and
/// the function abstains when retrieval yields no evidence. A future model
/// synthesizer can sit on top without weakening this evidence contract.
pub fn ask(db: &Db, request: &MemoryAskRequest) -> AppResult<MemoryAnswer> {
    if request.question.trim().chars().count() < 2 {
        return Err(crate::error::AppError::Io(std::io::Error::other(
            "question is too short",
        )));
    }
    if !matches!(
        request.domain.as_str(),
        "work" | "planphysique" | "personal" | "family" | "finance" | "research"
    ) {
        return Err(crate::error::AppError::Io(std::io::Error::other(
            "invalid memory domain",
        )));
    }

    let results = search(
        db,
        &request.question,
        Some(&request.domain),
        &MemorySearchOpts {
            include_stale: request.include_stale,
            limit: Some(5),
        },
    )?;
    if results.is_empty() {
        return Ok(MemoryAnswer {
            answer: "Non ho trovato evidenze sufficienti nel Second Brain per rispondere."
                .to_string(),
            citations: Vec::new(),
            warnings: Vec::new(),
            abstained: true,
        });
    }

    let mut citations = Vec::new();
    let mut warnings = Vec::new();
    for (index, result) in results.into_iter().take(3).enumerate() {
        let (content, _) = super::vault::read_file(&result.row.vault_path)?;
        let body = super::frontmatter::parse(&content)
            .map(|(_, body)| body)
            .unwrap_or(content);
        let excerpt = best_excerpt(&body, &request.question);
        if result.row.status == "stale" {
            warnings.push(format!(
                "La fonte [{}] è obsoleta e va verificata prima di agire.",
                index + 1
            ));
        }
        citations.push(MemoryCitation {
            id: result.row.id,
            number: index + 1,
            title: result.row.title,
            vault_path: result.row.vault_path,
            status: result.row.status,
            excerpt,
            score: result.score,
        });
    }
    let answer = citations
        .iter()
        .map(|citation| format!("{} [{}]", citation.excerpt, citation.number))
        .collect::<Vec<_>>()
        .join("\n\n");

    Ok(MemoryAnswer {
        answer,
        citations,
        warnings,
        abstained: false,
    })
}

fn score_row(row: &super::MemoryRow, bm25_normalized: f64) -> ScoredMemory {
    // Recency: exponential decay
    let age_days = parse_age_days(&row.updated_at);
    let hl = half_life_days(&row.mem_type);
    let recency = (-std::f64::consts::LN_2 * age_days / hl).exp();

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
