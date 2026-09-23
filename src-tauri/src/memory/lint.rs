use std::collections::{BTreeMap, BTreeSet};

use serde_json::json;

use crate::db::Db;
use crate::error::{AppError, AppResult};

use super::{MemoryLintFinding, MemoryLintReport, MemoryRow};

/// Caps keep every lint run bounded regardless of vault size: the report is
/// a prioritized digest, not an exhaustive dump.
const MAX_STALE_FINDINGS: usize = 6;
const MAX_CONTRADICTION_PAIRS: usize = 8;
const MAX_PAIR_BODY_CHARS: usize = 700;
const PAIR_SIMILARITY_THRESHOLD: f64 = 0.22;

struct Note {
    row: MemoryRow,
    related: Vec<String>,
    body: String,
}

/// The wiki-pattern "lint operation" adapted to this app's governance: the
/// pass is strictly read-only. Deterministic checks (broken links, orphans,
/// staleness) run in Rust; the optional deep pass asks the model to flag
/// contradictions between lexically-overlapping notes. Findings are audited
/// and returned for human review — lint never mutates the vault.
pub async fn run_lint(
    db: &Db,
    domain: Option<&str>,
    deep: bool,
) -> AppResult<MemoryLintReport> {
    super::index::ensure_tables(db)?;
    let rows = match domain {
        Some(domain) => super::index::list_by_domain(db, domain)?,
        None => super::index::list_all(db)?,
    };

    let mut notes = Vec::new();
    for row in rows {
        if row.status == "expired" {
            continue;
        }
        let Ok((content, _)) = super::vault::read_file(&row.vault_path) else {
            continue;
        };
        let (related, body) = match super::frontmatter::parse(&content) {
            Some((fm, body)) => (fm.related, body),
            None => (Vec::new(), content),
        };
        notes.push(Note { row, related, body });
    }

    let known_paths: BTreeSet<&str> = notes
        .iter()
        .map(|note| note.row.vault_path.as_str())
        .collect();
    let mut inbound: BTreeMap<&str, usize> = BTreeMap::new();
    for note in &notes {
        for target in &note.related {
            if let Some(path) = known_paths.get(target.as_str()) {
                *inbound.entry(path).or_default() += 1;
            }
        }
    }

    let mut findings = Vec::new();

    for note in &notes {
        for target in &note.related {
            if !known_paths.contains(target.as_str()) {
                findings.push(MemoryLintFinding {
                    kind: "broken_link".to_string(),
                    severity: "warning".to_string(),
                    paths: vec![note.row.vault_path.clone(), target.clone()],
                    detail: format!(
                        "'{}' links to '{}', which is not in the index (moved, expired, or mistyped).",
                        note.row.title, target
                    ),
                });
            }
        }
    }

    for note in &notes {
        if note.row.mem_type == "episode" || note.row.status != "active" {
            continue;
        }
        let has_outbound = !note.related.is_empty();
        let has_inbound = inbound
            .get(note.row.vault_path.as_str())
            .copied()
            .unwrap_or(0)
            > 0;
        if !has_outbound && !has_inbound && note.row.access_count == 0 {
            findings.push(MemoryLintFinding {
                kind: "orphan".to_string(),
                severity: "info".to_string(),
                paths: vec![note.row.vault_path.clone()],
                detail: format!(
                    "'{}' has no links in either direction and has never been retrieved.",
                    note.row.title
                ),
            });
        }
    }

    let stale: Vec<&Note> = notes
        .iter()
        .filter(|note| note.row.status == "stale")
        .collect();
    for note in stale.iter().take(MAX_STALE_FINDINGS) {
        findings.push(MemoryLintFinding {
            kind: "stale".to_string(),
            severity: "info".to_string(),
            paths: vec![note.row.vault_path.clone()],
            detail: format!(
                "'{}' is stale — confirm it is still true or supersede it.",
                note.row.title
            ),
        });
    }
    if stale.len() > MAX_STALE_FINDINGS {
        findings.push(MemoryLintFinding {
            kind: "stale".to_string(),
            severity: "info".to_string(),
            paths: Vec::new(),
            detail: format!(
                "{} more stale memories not listed — run maintenance and review the oldest first.",
                stale.len() - MAX_STALE_FINDINGS
            ),
        });
    }

    let mut model_tokens = None;
    if deep {
        let (contradictions, tokens) = contradiction_pass(&notes).await?;
        findings.extend(contradictions);
        model_tokens = tokens;
    }

    let report = MemoryLintReport {
        generated_at: chrono::Utc::now().to_rfc3339(),
        scanned: notes.len() as i64,
        findings,
        deep,
        model_tokens,
    };

    crate::audit::append_row(
        db,
        "memory-lint",
        "memory-lint",
        "memory_lint",
        &format!(
            "Memory lint: {} notes scanned, {} findings{}",
            report.scanned,
            report.findings.len(),
            if deep { " (deep)" } else { "" }
        ),
        &json!({
            "scanned": report.scanned,
            "findings": report.findings,
            "deep": deep,
        }),
        model_tokens,
        None,
    )?;

    Ok(report)
}

/// Model-assisted contradiction detection over lexically-overlapping pairs.
/// Bounded by pair count and per-note excerpt size; the model only ever sees
/// note content that is already in the vault, and its output is reduced to
/// pair indexes plus an explanation — it cannot introduce new paths.
async fn contradiction_pass(
    notes: &[Note],
) -> AppResult<(Vec<MemoryLintFinding>, Option<i64>)> {
    let mut pairs: Vec<(usize, usize, f64)> = Vec::new();
    for left in 0..notes.len() {
        for right in (left + 1)..notes.len() {
            let a = &notes[left];
            let b = &notes[right];
            if a.row.domain != b.row.domain
                || a.row.mem_type == "episode"
                || b.row.mem_type == "episode"
            {
                continue;
            }
            let left_text = format!("{} {}", a.row.title, take_chars(&a.body, 240));
            let right_text = format!("{} {}", b.row.title, take_chars(&b.body, 240));
            let similarity = super::pipeline::jaccard(&left_text, &right_text);
            if similarity >= PAIR_SIMILARITY_THRESHOLD {
                pairs.push((left, right, similarity));
            }
        }
    }
    pairs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    pairs.truncate(MAX_CONTRADICTION_PAIRS);
    if pairs.is_empty() {
        return Ok((Vec::new(), None));
    }

    let pair_blocks = pairs
        .iter()
        .enumerate()
        .map(|(index, (left, right, _))| {
            json!({
                "pair": index + 1,
                "noteA": {
                    "title": notes[*left].row.title,
                    "status": notes[*left].row.status,
                    "updated": notes[*left].row.updated_at,
                    "text": take_chars(&notes[*left].body, MAX_PAIR_BODY_CHARS),
                },
                "noteB": {
                    "title": notes[*right].row.title,
                    "status": notes[*right].row.status,
                    "updated": notes[*right].row.updated_at,
                    "text": take_chars(&notes[*right].body, MAX_PAIR_BODY_CHARS),
                },
            })
        })
        .collect::<Vec<_>>();

    let prompt = format!(
        "You are auditing a personal knowledge base for factual contradictions.\n\
         For each numbered pair below, decide whether the two notes make claims that \
         cannot both be true at the same time. Different topics, complementary details, \
         or one note being more specific are NOT contradictions.\n\
         Reply with STRICT JSON only, no markdown fences:\n\
         {{\"contradictions\":[{{\"pair\":<number>,\"explanation\":\"<one concise sentence>\"}}]}}\n\
         Return an empty array when nothing conflicts.\n\nPAIRS:\n{}",
        serde_json::to_string_pretty(&pair_blocks).unwrap_or_default()
    );

    let output = crate::harness::structured::run_read_only_json_with_progress(
        &prompt,
        |_| {},
        crate::harness::structured::no_cancel(),
    )
    .await?;

    let cleaned = output
        .text
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();
    let parsed: serde_json::Value = serde_json::from_str(cleaned)
        .map_err(|_| AppError::Io(std::io::Error::other("lint model returned invalid JSON")))?;

    let mut findings = Vec::new();
    if let Some(items) = parsed.get("contradictions").and_then(|v| v.as_array()) {
        for item in items {
            let Some(pair_number) = item.get("pair").and_then(|v| v.as_u64()) else {
                continue;
            };
            let Some((left, right, _)) = pairs.get((pair_number as usize).wrapping_sub(1)) else {
                continue;
            };
            let explanation = item
                .get("explanation")
                .and_then(|v| v.as_str())
                .unwrap_or("The two notes make mutually exclusive claims.");
            findings.push(MemoryLintFinding {
                kind: "contradiction".to_string(),
                severity: "warning".to_string(),
                paths: vec![
                    notes[*left].row.vault_path.clone(),
                    notes[*right].row.vault_path.clone(),
                ],
                detail: take_chars(explanation, 300),
            });
        }
    }
    Ok((findings, output.tokens))
}

fn take_chars(value: &str, max: usize) -> String {
    value.chars().take(max).collect()
}
