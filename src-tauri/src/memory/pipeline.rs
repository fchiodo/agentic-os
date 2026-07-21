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
        Regex::new(r"(?i)AKIA[0-9A-Z]{16}").unwrap(), // AWS access key
        Regex::new(r"(?i)-----BEGIN\s+(RSA\s+)?PRIVATE\s+KEY").unwrap(),
        Regex::new(r"(?i)(password|passwd|pwd)\s*[:=]\s*\S+").unwrap(),
        Regex::new(r"(?i)bearer\s+[A-Za-z0-9\-._~+/]+=*").unwrap(),
        Regex::new(r"(?i)eyJ[A-Za-z0-9_-]{10,}\.eyJ").unwrap(), // JWT
        Regex::new(r"[0-9a-fA-F]{64,}").unwrap(),               // long hex (API keys)
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

pub(crate) fn contains_secrets(text: &str) -> bool {
    // Documentation commonly contains obvious placeholders. Remove only
    // exact, well-known placeholders before scanning; real values still hit
    // the deterministic patterns below.
    let sanitized = [
        "YOUR_API_TOKEN",
        "YOUR_SIGNED_JWT",
        "YOUR_PASSWORD",
        "__TOKEN__",
        "<TOKEN>",
        "<PASSWORD>",
    ]
    .iter()
    .fold(text.to_string(), |value, placeholder| {
        value.replace(placeholder, "<documentation-placeholder>")
    });
    let sanitized = Regex::new(r"(?i)\bbearer\s+(api\s+)?token\b")
        .expect("static bearer prose regex")
        .replace_all(&sanitized, "bearer <token-description>");
    secrets_patterns()
        .iter()
        .any(|pattern| pattern.is_match(sanitized.as_ref()))
}

pub(crate) fn contains_prompt_injection(text: &str) -> bool {
    injection_patterns()
        .iter()
        .any(|pattern| pattern.is_match(text))
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

fn parse_lifecycle_date(value: &str) -> Option<chrono::NaiveDate> {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|value| value.date_naive())
        .ok()
        .or_else(|| chrono::NaiveDate::parse_from_str(value, "%Y-%m-%d").ok())
}

fn validate_lifecycle(request: &ManualSaveRequest) -> AppResult<()> {
    let parse_optional = |label: &str, value: Option<&String>| -> AppResult<Option<chrono::NaiveDate>> {
        value
            .map(|value| {
                parse_lifecycle_date(value).ok_or_else(|| {
                    AppError::Io(std::io::Error::other(format!(
                        "{label} must be an RFC 3339 timestamp or YYYY-MM-DD"
                    )))
                })
            })
            .transpose()
    };

    let valid_from = parse_optional("validFrom", request.valid_from.as_ref())?;
    let valid_until = parse_optional("validUntil", request.valid_until.as_ref())?;
    parse_optional("expires", request.expires.as_ref())?;
    if matches!((valid_from, valid_until), (Some(from), Some(until)) if from > until) {
        return Err(AppError::Io(std::io::Error::other(
            "validFrom must not be later than validUntil",
        )));
    }
    if request
        .stale_after_days
        .is_some_and(|days| !(1..=36_500).contains(&days))
    {
        return Err(AppError::Io(std::io::Error::other(
            "staleAfterDays must be between 1 and 36500",
        )));
    }
    if request
        .confidence
        .is_some_and(|confidence| !confidence.is_finite() || !(0.0..=1.0).contains(&confidence))
    {
        return Err(AppError::Io(std::io::Error::other(
            "confidence must be a finite number between 0 and 1",
        )));
    }
    Ok(())
}

/// Process a manual save request through the write pipeline.
pub fn process_manual_save(
    db: &Db,
    request: &ManualSaveRequest,
    source: &str,
) -> AppResult<super::MemoryWriteProposal> {
    process_manual_save_with_context(db, request, source, false, None)
}

/// Imported document facts are untrusted extraction output. Even in domains
/// where ordinary writes may auto-apply, import candidates always stop at the
/// proposal boundary for explicit human review.
pub(crate) fn process_import_candidate(
    db: &Db,
    request: &ManualSaveRequest,
    source: &str,
    import_id: &str,
) -> AppResult<super::MemoryWriteProposal> {
    process_manual_save_with_context(db, request, source, true, Some(import_id))
}

fn process_manual_save_with_context(
    db: &Db,
    request: &ManualSaveRequest,
    source: &str,
    force_approval: bool,
    import_id: Option<&str>,
) -> AppResult<super::MemoryWriteProposal> {
    super::index::ensure_tables(db)?;
    let mem_type = MemoryType::parse(&request.mem_type)
        .ok_or_else(|| AppError::Io(std::io::Error::other("invalid memory type")))?;
    let domain = match request.domain.as_str() {
        "work" => crate::control_models::Domain::Work,
        "planphysique" => crate::control_models::Domain::Planphysique,
        "personal" => crate::control_models::Domain::Personal,
        "family" => crate::control_models::Domain::Family,
        "finance" => crate::control_models::Domain::Finance,
        "research" => crate::control_models::Domain::Research,
        _ => {
            audit_gate_reject(db, source, "invalid domain", &request.title);
            return Err(AppError::Io(std::io::Error::other("invalid memory domain")));
        }
    };
    let title = request.title.trim();
    let body = request.body.trim();
    if title.is_empty() || title.chars().count() > 200 || body.is_empty() {
        audit_gate_reject(db, source, "invalid title or empty body", title);
        return Err(AppError::Io(std::io::Error::other(
            "title must be 1-200 characters and body must not be empty",
        )));
    }
    if let Err(error) = validate_lifecycle(request) {
        audit_gate_reject(db, source, "invalid lifecycle metadata", title);
        return Err(error);
    }

    let now = chrono::Utc::now().to_rfc3339();
    let provenance_source = request
        .source
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(source);
    let mut report = GateReport::new();

    let candidate_text = format!("{title}\n{body}");
    let has_secrets = contains_secrets(&candidate_text);
    report.check(
        "secrets",
        !has_secrets,
        if has_secrets {
            "REJECTED: secrets detected"
        } else {
            "passed"
        },
    );
    if has_secrets {
        audit_gate_reject(db, provenance_source, "secrets detected", title);
        return Err(AppError::Io(std::io::Error::other(
            "write gate rejected: secrets detected in content",
        )));
    }

    let has_injection = contains_prompt_injection(body);
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
        audit_gate_reject(db, provenance_source, "injection suspect", title);
        return Err(AppError::Io(std::io::Error::other(
            "write gate rejected: injection suspect",
        )));
    }

    if matches!(
        mem_type,
        MemoryType::Fact | MemoryType::Decision | MemoryType::Preference
    ) {
        let ok = body.chars().count() <= 1200;
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
            audit_gate_reject(db, provenance_source, "body too long", title);
            return Err(AppError::Io(std::io::Error::other(
                "write gate rejected: fact body must be <= 1200 chars",
            )));
        }
    }

    let prov_ok = provenance_source == "manual" || provenance_source.contains(':');
    report.check(
        "provenance",
        prov_ok,
        if prov_ok {
            "passed"
        } else {
            "REJECTED: missing provenance"
        },
    );
    if !prov_ok {
        audit_gate_reject(db, provenance_source, "unresolvable provenance", title);
        return Err(AppError::Io(std::io::Error::other(
            "write gate rejected: provenance must be 'manual' or a namespaced source reference",
        )));
    }

    let dedup_result = if let Some(old_id) = request.supersedes_id.as_ref() {
        let old = super::index::get_by_id(db, old_id)?
            .ok_or_else(|| AppError::Io(std::io::Error::other("superseded memory not found")))?;
        if old.domain != domain.as_str() {
            return Err(AppError::Io(std::io::Error::other(
                "cross-domain supersede is not allowed",
            )));
        }
        DedupResult::Contradicts(old_id.clone())
    } else if mem_type == MemoryType::Episode {
        DedupResult::None
    } else {
        check_dedup(db, title, body, domain.as_str())?
    };
    match &dedup_result {
        DedupResult::None => {
            report.check("dedup", true, "no duplicate found");
        }
        DedupResult::Similar(existing_id) => {
            report.check(
                "dedup",
                true,
                &format!("update of existing: {}", existing_id),
            );
        }
        DedupResult::Contradicts(existing_id) => {
            report.check("dedup", true, &format!("supersedes: {}", existing_id));
        }
    }

    let (op, supersedes_id) = match &dedup_result {
        DedupResult::None => (ProposalOp::Create, None),
        DedupResult::Similar(id) => (ProposalOp::Update, Some(id.clone())),
        DedupResult::Contradicts(id) => (ProposalOp::Supersede, Some(id.clone())),
    };

    let update_target = if op == ProposalOp::Update {
        let existing_id = supersedes_id
            .as_deref()
            .ok_or_else(|| AppError::Io(std::io::Error::other("update proposal has no target")))?;
        Some(
            super::index::get_by_id(db, existing_id)?
                .ok_or_else(|| AppError::Io(std::io::Error::other("updated memory not found")))?,
        )
    } else {
        None
    };
    let old_document = match update_target.as_ref() {
        Some(row) => Some(vault::read_file(&row.vault_path)?.0),
        None => None,
    };
    let supersede_document = if op == ProposalOp::Supersede {
        let old_id = supersedes_id
            .as_deref()
            .ok_or_else(|| AppError::Io(std::io::Error::other("supersede target is missing")))?;
        let old_row = super::index::get_by_id(db, old_id)?
            .ok_or_else(|| AppError::Io(std::io::Error::other("superseded memory not found")))?;
        Some(vault::read_file(&old_row.vault_path)?.0)
    } else {
        None
    };
    let base_content_hash = old_document
        .as_ref()
        .or(supersede_document.as_ref())
        .map(|content| crate::audit::compute_content_hash(content));
    let old_parsed = match old_document.as_ref() {
        Some(content) => Some(super::frontmatter::parse(content).ok_or_else(|| {
            AppError::Io(std::io::Error::other(
                "existing memory has invalid frontmatter; reindex or repair it before updating",
            ))
        })?),
        None => None,
    };

    let inherited_sensitivity = request
        .sensitivity
        .as_deref()
        .or_else(|| old_parsed.as_ref().map(|(fm, _)| fm.sensitivity.as_str()));
    let sensitivity = classify_sensitivity(inherited_sensitivity, &candidate_text)?;
    let mut confidence = request
        .confidence
        .or_else(|| old_parsed.as_ref().map(|(fm, _)| fm.confidence))
        .unwrap_or(0.8)
        .clamp(0.0, 1.0);
    let preference_is_inferred = mem_type == MemoryType::Preference
        && provenance_source != "manual"
        && !provenance_source.starts_with("meeting:");
    if preference_is_inferred {
        confidence = confidence.min(0.5);
    }
    report.check(
        "attribution",
        true,
        if preference_is_inferred {
            "inferred preference: confidence downgraded and approval required"
        } else {
            "passed"
        },
    );

    let (id, vault_path, created, confirmations, tags, final_body) =
        if let (Some(existing), Some((old_fm, old_body))) =
            (update_target.as_ref(), old_parsed.as_ref())
        {
            let mut tags = old_fm.tags.clone();
            for tag in normalize_tags(&request.tags) {
                if !tags.contains(&tag) {
                    tags.push(tag);
                }
            }
            (
                existing.id.clone(),
                existing.vault_path.clone(),
                old_fm.created.clone(),
                old_fm.confirmations.unwrap_or(0) + 1,
                tags,
                merge_bodies(old_body, body, mem_type),
            )
        } else {
            let id = Uuid::new_v4().to_string();
            let slug = slugify(title);
            let mut file_name = if mem_type == MemoryType::Episode {
                format!(
                    "{}-{}-{}",
                    chrono::Utc::now().format("%Y-%m-%d"),
                    slug,
                    &id[..8]
                )
            } else {
                slug
            };
            let candidate_path = format!(
                "{}/{}/{}.md",
                domain.as_str(),
                subdir_for(mem_type),
                file_name
            );
            if op == ProposalOp::Supersede || vault::file_exists(&candidate_path)? {
                file_name = format!("{}-{}", file_name, &id[..8]);
            }
            (
                id,
                format!(
                    "{}/{}/{}.md",
                    domain.as_str(),
                    subdir_for(mem_type),
                    file_name
                ),
                now.clone(),
                1,
                normalize_tags(&request.tags),
                body.to_string(),
            )
        };

    let path_ok = vault_path.starts_with(&format!("{}/", domain.as_str()));
    report.check(
        "domain_fence",
        path_ok,
        if path_ok { "passed" } else { "REJECTED" },
    );
    if !path_ok {
        return Err(AppError::Io(std::io::Error::other(
            "write gate rejected: vault path outside domain",
        )));
    }

    let expires = request
        .expires
        .clone()
        .or_else(|| old_parsed.as_ref().and_then(|(fm, _)| fm.expires.clone()))
        .or_else(|| {
            mem_type.default_ttl_days().map(|days| {
                (chrono::Utc::now() + chrono::Duration::days(days))
                    .format("%Y-%m-%d")
                    .to_string()
            })
        });
    let fm = MemoryFrontmatter {
        id: id.clone(),
        mem_type,
        domain: domain.as_str().to_string(),
        title: title.to_string(),
        created,
        updated: now.clone(),
        provenance: Provenance {
            source: provenance_source.to_string(),
            ts: now.clone(),
        },
        confidence,
        sensitivity,
        valid_from: request.valid_from.clone().or_else(|| {
            old_parsed
                .as_ref()
                .and_then(|(fm, _)| fm.valid_from.clone())
        }),
        valid_until: request.valid_until.clone().or_else(|| {
            old_parsed
                .as_ref()
                .and_then(|(fm, _)| fm.valid_until.clone())
        }),
        stale_after_days: request
            .stale_after_days
            .or_else(|| old_parsed.as_ref().and_then(|(fm, _)| fm.stale_after_days))
            .or_else(|| mem_type.default_stale_after_days()),
        last_confirmed: Some(now.clone()),
        confirmations: Some(confirmations),
        expires,
        tags,
    };
    let content = super::frontmatter::serialize(&fm, &final_body);
    let old_content = old_document
        .as_deref()
        .or(supersede_document.as_deref())
        .unwrap_or_default();
    let unified_diff = make_unified_diff(old_content, &content, &vault_path);

    let requires_approval = force_approval
        || !matches!(
            domain,
            crate::control_models::Domain::Work | crate::control_models::Domain::Research
        )
        || sensitivity == Sensitivity::Sensitive
        || op == ProposalOp::Supersede
        || preference_is_inferred;
    let auto_apply = !requires_approval && matches!(op, ProposalOp::Create | ProposalOp::Update);

    let proposal_id = Uuid::new_v4().to_string();
    let provenance_str = serde_json::to_string(&fm.provenance).unwrap_or_default();
    let task_id = provenance_source.strip_prefix("task:").map(str::to_string);

    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO memory_proposals (
                id, task_id, vault_path, domain, kind, op, supersedes_id,
                sensitivity, unified_diff, new_content, provenance,
                gate_report, requires_approval, status, created_at, decided_at,
                base_content_hash, import_id
            ) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,'pending',?14,NULL,?15,?16)",
            params![
                proposal_id,
                task_id,
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
                now,
                base_content_hash,
                import_id,
            ],
        )?;
        Ok(())
    })?;

    let proposal = super::proposals::get_by_id(db, &proposal_id)?
        .ok_or_else(|| AppError::Io(std::io::Error::other("memory proposal vanished")))?;
    if auto_apply {
        super::persist::apply_memory_proposal(db, &proposal, ProposalStatus::AutoApplied.as_str())?;
        return super::proposals::get_by_id(db, &proposal_id)?
            .ok_or_else(|| AppError::Io(std::io::Error::other("applied proposal vanished")));
    }
    Ok(proposal)
}

/// Admission boundary for connector/model extraction. The extractor is
/// deliberately outside the trusted core: it may submit at most ten typed
/// candidates and every candidate independently passes the same deterministic
/// gate as a manual save. One bad candidate cannot suppress valid siblings.
pub fn process_ingest_batch(
    db: &Db,
    request: &super::MemoryIngestRequest,
) -> AppResult<super::MemoryIngestResult> {
    if request.candidates.is_empty() || request.candidates.len() > 10 {
        return Err(AppError::Io(std::io::Error::other(
            "ingestion requires between 1 and 10 candidates",
        )));
    }
    if !request.source.contains(':') {
        return Err(AppError::Io(std::io::Error::other(
            "ingestion source must be a namespaced reference",
        )));
    }

    let mut proposals = Vec::new();
    let mut rejected = Vec::new();
    for (candidate_index, candidate) in request.candidates.iter().enumerate() {
        let manual = ManualSaveRequest {
            domain: request.domain.clone(),
            mem_type: candidate.mem_type.clone(),
            title: candidate.title.clone(),
            body: candidate.body.clone(),
            tags: candidate.tags.clone(),
            sensitivity: candidate.sensitivity.clone(),
            source: Some(request.source.clone()),
            confidence: candidate.confidence,
            valid_from: candidate.valid_from.clone(),
            valid_until: candidate.valid_until.clone(),
            stale_after_days: candidate.stale_after_days,
            expires: candidate.expires.clone(),
            supersedes_id: candidate.supersedes_id.clone(),
        };
        match process_manual_save(db, &manual, &request.source) {
            Ok(proposal) => proposals.push(proposal),
            Err(error) => rejected.push(super::MemoryIngestFailure {
                index: candidate_index,
                title: candidate.title.clone(),
                error: error.to_string(),
            }),
        }
    }

    Ok(super::MemoryIngestResult {
        proposals,
        rejected,
    })
}

// ---------------------------------------------------------------------------
// Run capture (MEMORY-SPEC §4 source 1 — deterministic v1)
// ---------------------------------------------------------------------------

/// Capture a completed run as an episodic memory. Deterministic v1: the
/// episode body is built from structured task data plus the captured final
/// agent output; it still traverses the deterministic admission gate.
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
        sensitivity: None,
        source: Some(format!("task:{task_id}")),
        confidence: Some(1.0),
        valid_from: None,
        valid_until: None,
        stale_after_days: None,
        // Run traces are short-lived working evidence; durable facts and
        // decisions extracted from them live independently.
        expires: Some(
            (chrono::Utc::now() + chrono::Duration::days(30))
                .format("%Y-%m-%d")
                .to_string(),
        ),
        supersedes_id: None,
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

    let existing_content = vault::read_skill_file(&skill_path).ok();
    let base_content_hash = existing_content
        .as_ref()
        .map(|content| crate::audit::compute_content_hash(content));
    let skill_op = if existing_content.is_some() { "update" } else { "create" };
    let unified_diff = make_unified_diff(
        existing_content.as_deref().unwrap_or_default(),
        &content,
        &skill_path,
    );
    let proposal_id = Uuid::new_v4().to_string();
    let provenance =
        serde_json::json!({ "source": format!("distill:{task_id}"), "ts": now }).to_string();
    let gate_report = r#"{"checks":[{"name":"procedural_approval","passed":true,"detail":"skills always require human approval"}],"passed":true}"#;

    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO memory_proposals (
                id, task_id, vault_path, domain, kind, op, supersedes_id,
                sensitivity, unified_diff, new_content, provenance,
                gate_report, requires_approval, status, created_at, decided_at,
                base_content_hash
            ) VALUES (?1,?2,?3,?4,?5,?6,NULL,'normal',?7,?8,?9,?10,1,'pending',?11,NULL,?12)",
            params![
                proposal_id,
                task_id,
                skill_path,
                domain,
                ProposalKind::Skill.as_str(),
                skill_op,
                unified_diff,
                content,
                provenance,
                gate_report,
                now,
                base_content_hash,
            ],
        )?;
        Ok(())
    })?;

    super::proposals::get_by_id(db, &proposal_id)?.ok_or_else(|| {
        AppError::Io(std::io::Error::other(
            "skill proposal vanished after insert",
        ))
    })
}

// ---------------------------------------------------------------------------
// Dedup
// ---------------------------------------------------------------------------

enum DedupResult {
    None,
    Similar(String),
    Contradicts(String),
}

fn classify_sensitivity(suggested: Option<&str>, text: &str) -> AppResult<Sensitivity> {
    if let Some(value) = suggested {
        if !matches!(value, "normal" | "sensitive") {
            return Err(AppError::Io(std::io::Error::other("invalid sensitivity")));
        }
    }
    let sensitive_pattern = Regex::new(
        r"(?i)\b(salary|stipendio|ral|iban|bank account|conto corrente|health|salute|medical|medico|diagnos|disabilit|legal case|contenzioso|codice fiscale|social security)\b",
    )
    .expect("static sensitivity regex");
    Ok(
        if suggested == Some("sensitive") || sensitive_pattern.is_match(text) {
            Sensitivity::Sensitive
        } else {
            Sensitivity::Normal
        },
    )
}

fn normalize_tags(tags: &[String]) -> Vec<String> {
    let mut normalized = Vec::new();
    let repeated_dashes = Regex::new(r"-{2,}").expect("static tag regex");
    for tag in tags {
        let clean = tag
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
            .collect::<String>();
        let clean = repeated_dashes
            .replace_all(clean.trim_matches('-'), "-")
            .to_string();
        if !clean.is_empty() && !normalized.contains(&clean) {
            normalized.push(clean);
        }
    }
    normalized.truncate(20);
    normalized
}

fn merge_bodies(old: &str, new: &str, mem_type: MemoryType) -> String {
    let old = old.trim();
    let new = new.trim();
    if old == new || old.contains(new) {
        return old.to_string();
    }
    if new.contains(old) {
        return new.to_string();
    }
    let merged = format!("{old}\n\nUpdate:\n{new}");
    if matches!(
        mem_type,
        MemoryType::Fact | MemoryType::Decision | MemoryType::Preference
    ) && merged.chars().count() > 1200
    {
        // Git retains the previous wording; the active atomic memory stays
        // within its durability limit.
        new.to_string()
    } else {
        merged
    }
}

fn normalized_terms(value: &str) -> std::collections::BTreeSet<String> {
    value
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|term| term.chars().count() > 1)
        .map(str::to_string)
        .collect()
}

fn jaccard(left: &str, right: &str) -> f64 {
    let left = normalized_terms(left);
    let right = normalized_terms(right);
    if left.is_empty() || right.is_empty() {
        return 0.0;
    }
    let intersection = left.intersection(&right).count() as f64;
    let union = left.union(&right).count() as f64;
    intersection / union
}

fn check_dedup(db: &Db, title: &str, _body: &str, domain: &str) -> AppResult<DedupResult> {
    // FTS search for similar titles in same domain. Join goes through
    // m.rowid (the FTS rowid mirrors memories.rowid, not the TEXT uuid),
    // and the title is sanitized into a safe MATCH expression.
    let Some(match_expr) = super::index::fts_match_expr(title) else {
        return Ok(DedupResult::None);
    };

    let results: Vec<(String, String)> = db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT m.id, m.title
             FROM memories_fts f
             JOIN memories m ON m.rowid = f.rowid
             WHERE memories_fts MATCH ?1 AND m.domain = ?2 AND m.status != 'expired'
             ORDER BY bm25(memories_fts)
             LIMIT 10",
        )?;

        let rows = stmt
            .query_map(params![match_expr, domain], |row| {
                let id: String = row.get(0)?;
                let candidate_title: String = row.get(1)?;
                Ok((id, candidate_title))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    })?;

    let normalized_title = normalized_terms(title)
        .into_iter()
        .collect::<Vec<_>>()
        .join(" ");
    if let Some((id, _, _)) = results
        .into_iter()
        .map(|(id, candidate_title)| {
            let normalized_candidate = normalized_terms(&candidate_title)
                .into_iter()
                .collect::<Vec<_>>()
                .join(" ");
            let score = if normalized_candidate == normalized_title {
                1.0
            } else {
                jaccard(title, &candidate_title)
            };
            (id, candidate_title, score)
        })
        .filter(|(_, _, score)| *score >= 0.82)
        .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
    {
        return Ok(DedupResult::Similar(id));
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
