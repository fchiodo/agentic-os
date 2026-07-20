use sha2::Digest;
use regex::Regex;
use rusqlite::params;
use uuid::Uuid;

use crate::db::Db;
use crate::error::{AppError, AppResult};

use super::vault;
use super::{
    ManualSaveRequest, MemoryFrontmatter, MemoryType, ProposalKind, ProposalOp, ProposalStatus,
    Provenance, Sensitivity,
};

// ---------------------------------------------------------------------------
// Secrets gate patterns
// ---------------------------------------------------------------------------

fn secrets_patterns() -> Vec<Regex> {
    vec![
        Regex::new(r"(?i)AKIA[0-9A-Z]{16}").unwrap(),              // AWS access key
        Regex::new(r"(?i)-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY").unwrap(),
        Regex::new(r"(?i)(password|passwd|pwd)\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"(?i)bearer\s+[A-Za-z0-9\-._~+/]+=*").unwrap(),
        Regex::new(r"(?i)eyJ[A-Za-z0-9_-]{10,}\.eyJ").unwrap(),   // JWT
        Regex::new(r"[0-9a-fA-F]{64,}").unwrap(),                  // long hex (API keys)
    ]
}

fn injection_patterns() -> Vec<Regex> {
    vec![
        Regex::new(r"(?i)ignore\s+previous\s+instructions").unwrap(),
        Regex::new(r"(?i)always\s+run").unwrap(),
        Regex::new(r"(?i)you\s+are\s+now").unwrap(),
        Regex::new(r"(?i)system\s*prompt").unwrap(),
    ]
}

// ---------------------------------------------------------------------------
// Gate report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Serialize)]
struct GateCheck {
    name: String,
    passed: bool,
    detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct GateReport {
    checks: Vec<GateCheck>,
    passed: bool,
}

impl GateReport {
    fn new() -> Self {
        Self {
            checks: Vec::new(),
            passed: true,
        }
    }

    fn check(&mut self, name: &str, passed: bool, detail: &str) {
        self.checks.push(GateCheck {
            name: name.to_string(),
            passed,
            detail: detail.to_string(),
        });
        if !passed {
            self.passed = false;
        }
    }

    fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Produce a real unified diff for the proposal UI. `old` is empty for
/// op=create; for update/supersede it is the current file content.
fn make_unified_diff(old: &str, new: &str, path: &str) -> String {
    similar::TextDiff::from_lines(old, new)
        .unified_diff()
        .context_radius(3)
        .header(&format!("a/{path}"), &format!("b/{path}"))
        .to_string()
}

/// Every gate rejection leaves a permanent trace (MEMORY-SPEC §5.2 /
/// M2 acceptance: "rejected with audit row"). The run id groups all
/// gate rejections so they are queryable as a class.
fn audit_gate_reject(db: &Db, source: &str, reason: &str, title: &str) {
    let detail = serde_json::json!({
        "reason": reason,
        "title": title,
        "source": source,
    });
    if let Err(err) = crate::audit::append_row(
        db,
        "memory-gate",
        source,
        "policy_decision",
        &format!("Memory write rejected by gate: {reason}"),
        &detail,
        None,
        None,
    ) {
        log::error!("failed to audit gate rejection: {err}");
    }
}

/// Vault subdirectory per memory type — decisions and episodes get their
/// own folders (Obsidian navigability), everything else lands in memories/.
fn subdir_for(mem_type: MemoryType) -> &'static str {
    match mem_type {
        MemoryType::Decision => "decisions",
        MemoryType::Episode => "episodes",
        _ => "memories",
    }
}

/// Process a manual save request through the write pipeline.
pub fn process_manual_save(
    db: &Db,
    request: &ManualSaveRequest,
    source: &str,
) -> AppResult<super::MemoryWriteProposal> {
    // The pipeline can be the very first memory code that runs in a fresh
    // install (e.g. run capture after the first completed task) — never
    // assume another command created the tables already.
    super::index::ensure_tables(db)?;

    let mem_type = MemoryType::parse(&request.mem_type)
        .ok_or_else(|| AppError::Io(std::io::Error::other("invalid memory type")))?;

    let domain = crate::control_models::Domain::parse(&request.domain);

    // Build frontmatter
    let now = chrono::Utc::now().to_rfc3339();
    let id = Uuid::new_v4().to_string();
    let slug = slugify(&request.title);
    let vault_path = format!("{}/{}/{}.md", domain.as_str(), subdir_for(mem_type), slug);

    let fm = MemoryFrontmatter {
        id: id.clone(),
        mem_type,
        domain: domain.as_str().to_string(),
        title: request.title.clone(),
        created: now.clone(),
        updated: now.clone(),
        provenance: Provenance {
            source: source.to_string(),
            ts: now.clone(),
        },
        confidence: 0.8,
        sensitivity: Sensitivity::Normal,
        valid_from: None,
        valid_until: None,
        stale_after_days: mem_type.default_stale_after_days(),
        last_confirmed: Some(now.clone()),
        confirmations: Some(1),
        expires: mem_type
            .default_ttl_days()
            .map(|d| {
                (chrono::Utc::now() + chrono::Duration::days(d))
                    .format("%Y-%m-%d")
                    .to_string()
            }),
        tags: request.tags.clone(),
    };

    let content = super::frontmatter::serialize(&fm, &request.body);

    // Run through the gate
    let mut report = GateReport::new();

    // Secrets check
    let has_secrets = secrets_patterns().iter().any(|p| p.is_match(&request.body));
    report.check("secrets", !has_secrets, if has_secrets { "REJECTED: secrets detected" } else { "passed" });

    if has_secrets {
        audit_gate_reject(db, source, "secrets detected", &request.title);
        return Err(AppError::Io(std::io::Error::other(
            "write gate rejected: secrets detected in content",
        )));
    }

    // Injection check
    let has_injection = injection_patterns().iter().any(|p| p.is_match(&request.body));
    report.check(
        "injection",
        !has_injection,
        if has_injection {
            "REJECTED: injection suspect"
        } else {
            "passed"
        },
    );

    if has_injection {
        audit_gate_reject(db, source, "injection suspect", &request.title);
        return Err(AppError::Io(std::io::Error::other(
            "write gate rejected: injection suspect",
        )));
    }

    // Body length check for facts
    if matches!(mem_type, MemoryType::Fact | MemoryType::Decision | MemoryType::Preference) {
        let ok = request.body.len() <= 1200;
        report.check(
            "body_length",
            ok,
            if ok {
                "passed"
            } else {
                "REJECTED: body too long for fact/decision/preference"
            },
        );
        if !ok {
            audit_gate_reject(db, source, "body too long", &request.title);
            return Err(AppError::Io(std::io::Error::other(
                "write gate rejected: fact body must be <= 1200 chars",
            )));
        }
    }

    // Domain fence check (path must be under domain dir)
    let path_ok = vault_path.starts_with(&format!("{}/", domain.as_str()));
    report.check("domain_fence", path_ok, if path_ok { "passed" } else { "REJECTED: path outside domain" });
    if !path_ok {
        return Err(AppError::Io(std::io::Error::other(
            "write gate rejected: vault path outside domain",
        )));
    }

    // Provenance check
    let prov_ok = !source.is_empty();
    report.check("provenance", prov_ok, if prov_ok { "passed" } else { "REJECTED: missing provenance" });
    if !prov_ok {
        return Err(AppError::Io(std::io::Error::other(
            "write gate rejected: missing provenance",
        )));
    }

    // Dedup check
    let dedup_result = check_dedup(db, &request.title, &request.body, domain.as_str())?;
    match &dedup_result {
        DedupResult::None => {
            report.check("dedup", true, "no duplicate found");
        }
        DedupResult::Similar(existing_id) => {
            report.check("dedup", true, &format!("update of existing: {}", existing_id));
        }
        DedupResult::Contradicts(existing_id) => {
            report.check("dedup", true, &format!("supersedes: {}", existing_id));
        }
    }

    // Determine approval requirement
    let requires_approval = matches!(
        domain,
        crate::control_models::Domain::Personal
            | crate::control_models::Domain::Family
            | crate::control_models::Domain::Finance
    );

    let (op, supersedes_id) = match &dedup_result {
        DedupResult::None => (ProposalOp::Create, None),
        DedupResult::Similar(id) => (ProposalOp::Update, Some(id.clone())),
        DedupResult::Contradicts(id) => (ProposalOp::Supersede, Some(id.clone())),
    };

    // If auto-eligible (work/research, normal sensitivity, create/update)
    let auto_apply = !requires_approval
        && matches!(op, ProposalOp::Create | ProposalOp::Update)
        && fm.sensitivity == Sensitivity::Normal;

    let status = if auto_apply {
        // Auto-apply: write file + index immediately
        vault::ensure_vault()?;
        vault::write_file(&vault_path, &content)?;
        let _ = vault::git_commit(&format!(
            "mem({}): {} {} [{}]",
            domain.as_str(),
            op.as_str(),
            slug,
            source
        ));

        // Upsert index
        let content_hash = sha2::Sha256::digest(content.as_bytes());
        let hash_hex = format!("{:x}", content_hash);
        let row = super::MemoryRow {
            id: id.clone(),
            vault_path: vault_path.clone(),
            domain: domain.as_str().to_string(),
            mem_type: mem_type.as_str().to_string(),
            title: request.title.clone(),
            summary: Some(request.body.chars().take(280).collect()),
            sensitivity: fm.sensitivity.as_str().to_string(),
            confidence: fm.confidence,
            created_at: now.clone(),
            updated_at: now.clone(),
            valid_from: None,
            valid_until: None,
            stale_after_days: mem_type.default_stale_after_days(),
            last_confirmed_at: Some(now.clone()),
            confirmation_count: 1,
            last_accessed_at: None,
            access_count: 0,
            expires_at: fm.expires.clone(),
            provenance: serde_json::to_string(&fm.provenance).unwrap_or_default(),
            content_hash: hash_hex,
            status: "active".to_string(),
        };
        let _ = super::index::upsert(db, &row, &request.body, &request.tags);

        ProposalStatus::AutoApplied
    } else {
        ProposalStatus::Pending
    };

    // Real unified diff for the approvals UI: empty base for create,
    // current file content for update/supersede.
    let old_content = supersedes_id
        .as_ref()
        .and_then(|existing_id| super::index::get_by_id(db, existing_id).ok().flatten())
        .and_then(|existing| vault::read_file(&existing.vault_path).ok())
        .map(|(content, _)| content)
        .unwrap_or_default();
    let unified_diff = make_unified_diff(&old_content, &content, &vault_path);

    // Save proposal
    let proposal_id = Uuid::new_v4().to_string();
    let provenance_str = serde_json::to_string(&fm.provenance).unwrap_or_default();

    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO memory_proposals (
                id, task_id, vault_path, domain, kind, op, supersedes_id,
                sensitivity, unified_diff, new_content, provenance,
                gate_report, requires_approval, status, created_at, decided_at
            ) VALUES (?1,NULL,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,NULL)",
            params![
                proposal_id,
                vault_path,
                domain.as_str(),
                ProposalKind::Memory.as_str(),
                op.as_str(),
                supersedes_id,
                fm.sensitivity.as_str(),
                unified_diff,
                content,
                provenance_str,
                report.to_json(),
                requires_approval as i64,
                status.as_str(),
                now,
            ],
        )?;
        Ok(())
    })?;

    Ok(super::MemoryWriteProposal {
        id: proposal_id,
        task_id: None,
        vault_path,
        domain: domain.as_str().to_string(),
        kind: ProposalKind::Memory.as_str().to_string(),
        op: op.as_str().to_string(),
        supersedes_id,
        sensitivity: fm.sensitivity.as_str().to_string(),
        unified_diff,
        new_content: content,
        provenance: provenance_str,
        gate_report: report.to_json(),
        requires_approval,
        status: status.as_str().to_string(),
        created_at: now,
        decided_at: None,
    })
}

// ---------------------------------------------------------------------------
// Run capture (MEMORY-SPEC §4 source 1 — deterministic v1)
// ---------------------------------------------------------------------------

/// Capture a completed run as an episodic memory. Deterministic v1: the
/// episode body is built from structured task data (goal, outcome, steps),
/// never from free-form model output, so it is gate-safe by construction.
/// LLM fact extraction (up to 10 typed candidates) plugs in here once the
/// model gateway lands — this function is its call site.
pub fn process_run_capture(
    db: &Db,
    task_id: &str,
    domain: &str,
    title: &str,
    goal: &str,
    outcome: &str,
) -> AppResult<Option<super::MemoryWriteProposal>> {
    // Capture policy: on for work/research only until Phase 5 (§4).
    if !matches!(domain, "work" | "research") {
        return Ok(None);
    }

    let body = format!(
        "Run outcome: {outcome}\n\nGoal:\n{goal}\n\nCaptured automatically from task {task_id}."
    );

    let request = ManualSaveRequest {
        domain: domain.to_string(),
        mem_type: "episode".to_string(),
        title: format!("Run: {title}"),
        body,
        tags: vec!["run".to_string()],
    };

    match process_manual_save(db, &request, &format!("task:{task_id}")) {
        Ok(proposal) => Ok(Some(proposal)),
        // A gate rejection on capture must never fail the task itself —
        // it is already audited by audit_gate_reject.
        Err(err) => {
            log::warn!("run capture skipped for task {task_id}: {err}");
            Ok(None)
        }
    }
}

// ---------------------------------------------------------------------------
// Skill distillation (MEMORY-SPEC §4 source 4)
// ---------------------------------------------------------------------------

/// Distill a completed run into a candidate skill. Always a pending
/// proposal — procedures are never persisted without human approval
/// (anti-pattern #1 in the spec). On approval the file lands under the
/// skills root the harnesses consume, not in the vault.
pub fn process_skill_distill(
    db: &Db,
    task_id: &str,
    domain: &str,
    title: &str,
    goal: &str,
    step_titles: &[String],
) -> AppResult<super::MemoryWriteProposal> {
    super::index::ensure_tables(db)?;
    let slug = slugify(title);
    let skill_path = format!("{slug}/SKILL.md");
    let now = chrono::Utc::now().to_rfc3339();

    let steps_md = if step_titles.is_empty() {
        "1. (reconstruct from the source run trace)".to_string()
    } else {
        step_titles
            .iter()
            .enumerate()
            .map(|(i, s)| format!("{}. {}", i + 1, s))
            .collect::<Vec<_>>()
            .join("\n")
    };

    let content = format!(
        "---\nname: {slug}\ndescription: Distilled from a successful run — {title}\nprovenance: task:{task_id}\ndistilled: {now}\n---\n\n# {title}\n\n## When to use\n\nUse this skill for tasks equivalent to the run it was distilled from.\n\n## Goal shape\n\n{goal}\n\n## Procedure\n\n{steps_md}\n\n## Verification\n\nReview the source run trace in Audit (task {task_id}) before first reuse.\n"
    );

    let unified_diff = make_unified_diff("", &content, &skill_path);
    let proposal_id = Uuid::new_v4().to_string();
    let provenance = serde_json::json!({ "source": format!("distill:{task_id}"), "ts": now })
        .to_string();
    let gate_report = r#"{"checks":[{"name":"procedural_approval","passed":true,"detail":"skills always require human approval"}],"passed":true}"#;

    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO memory_proposals (
                id, task_id, vault_path, domain, kind, op, supersedes_id,
                sensitivity, unified_diff, new_content, provenance,
                gate_report, requires_approval, status, created_at, decided_at
            ) VALUES (?1,?2,?3,?4,?5,'create',NULL,'normal',?6,?7,?8,?9,1,'pending',?10,NULL)",
            params![
                proposal_id,
                task_id,
                skill_path,
                domain,
                ProposalKind::Skill.as_str(),
                unified_diff,
                content,
                provenance,
                gate_report,
                now,
            ],
        )?;
        Ok(())
    })?;

    super::proposals::get_by_id(db, &proposal_id)?
        .ok_or_else(|| AppError::Io(std::io::Error::other("skill proposal vanished after insert")))
}

// ---------------------------------------------------------------------------
// Dedup
// ---------------------------------------------------------------------------

enum DedupResult {
    None,
    Similar(String),
    // Only the extractor path (task-run capture, MEMORY-SPEC §4 source 1)
    // can detect contradictions; manual saves cannot. Wired up when the
    // extraction pipeline lands.
    #[allow(dead_code)]
    Contradicts(String),
}

fn check_dedup(
    db: &Db,
    title: &str,
    _body: &str,
    domain: &str,
) -> AppResult<DedupResult> {
    // FTS search for similar titles in same domain. Join goes through
    // m.rowid (the FTS rowid mirrors memories.rowid, not the TEXT uuid),
    // and the title is sanitized into a safe MATCH expression.
    let Some(match_expr) = super::index::fts_match_expr(title) else {
        return Ok(DedupResult::None);
    };

    let results: Vec<(String, f64)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT m.id, bm25(memories_fts) as rank
             FROM memories_fts f
             JOIN memories m ON m.rowid = f.rowid
             WHERE memories_fts MATCH ?1 AND m.domain = ?2 AND m.status != 'expired'
             ORDER BY rank
             LIMIT 3",
        )?;

        let rows = stmt
            .query_map(params![match_expr, domain], |row| {
                let id: String = row.get(0)?;
                let rank: f64 = row.get(1)?;
                let normalized = 1.0 / (1.0 + rank.abs());
                Ok((id, normalized))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    })?;

    if let Some((id, score)) = results.first() {
        if *score > 0.82 {
            return Ok(DedupResult::Similar(id.clone()));
        }
    }

    Ok(DedupResult::None)
}

fn slugify(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else if c == ' ' || c == '-' || c == '_' {
                '-'
            } else {
                '\0'
            }
        })
        .filter(|c| *c != '\0')
        .collect();

    // Collapse multiple dashes
    let re = Regex::new(r"-{2,}").unwrap();
    let slug = re.replace_all(&slug, "-");
    let slug = slug.trim_matches('-');

    if slug.is_empty() {
        format!("mem-{}", chrono::Utc::now().timestamp())
    } else {
        slug.to_string()
    }
}
