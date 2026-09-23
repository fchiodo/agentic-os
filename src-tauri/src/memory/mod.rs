pub mod consolidation;
pub mod context;
pub mod email_extraction;
pub mod frontmatter;
pub mod importer;
pub mod index;
pub mod lint;
pub mod maintenance;
pub mod operations;
pub mod pdf_extraction;
pub mod persist;
pub mod pipeline;
pub mod proposals;
pub mod retrieval;
pub mod vault;

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Fact,
    Decision,
    Preference,
    Entity,
    Episode,
    /// A verified Ask answer promoted to a first-class note. Links back to
    /// the memories it cited, so answered questions compound instead of
    /// being re-derived from scratch on every query.
    Synthesis,
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::Fact => "fact",
            MemoryType::Decision => "decision",
            MemoryType::Preference => "preference",
            MemoryType::Entity => "entity",
            MemoryType::Episode => "episode",
            MemoryType::Synthesis => "synthesis",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fact" => Some(MemoryType::Fact),
            "decision" => Some(MemoryType::Decision),
            "preference" => Some(MemoryType::Preference),
            "entity" => Some(MemoryType::Entity),
            "episode" => Some(MemoryType::Episode),
            "synthesis" => Some(MemoryType::Synthesis),
            _ => None,
        }
    }

    /// Per-type default staleness in days. None means never goes stale.
    pub fn default_stale_after_days(&self) -> Option<i64> {
        match self {
            MemoryType::Fact => Some(180),
            MemoryType::Decision => None,
            MemoryType::Preference => Some(365),
            MemoryType::Entity => Some(365),
            MemoryType::Episode => Some(90),
            // Derived knowledge decays with its sources: same horizon as facts.
            MemoryType::Synthesis => Some(180),
        }
    }

    /// Default hard TTL in days for episodes.
    pub fn default_ttl_days(&self) -> Option<i64> {
        match self {
            MemoryType::Episode => Some(90),
            _ => None,
        }
    }
}

// Reserved for the context-builder integration (MEMORY-SPEC M4): typed
// status handling replaces the string comparisons currently used in
// retrieval.rs and the commands layer.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Stale,
    Expired,
}

#[allow(dead_code)]
impl MemoryStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryStatus::Active => "active",
            MemoryStatus::Stale => "stale",
            MemoryStatus::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "stale" => MemoryStatus::Stale,
            "expired" => MemoryStatus::Expired,
            _ => MemoryStatus::Active,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Normal,
    Sensitive,
}

impl Sensitivity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Sensitivity::Normal => "normal",
            Sensitivity::Sensitive => "sensitive",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "sensitive" => Sensitivity::Sensitive,
            _ => Sensitivity::Normal,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Provenance {
    pub source: String,
    pub ts: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryFrontmatter {
    pub id: String,
    pub mem_type: MemoryType,
    pub domain: String,
    pub title: String,
    pub created: String,
    pub updated: String,
    pub provenance: Provenance,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<String>,
    pub confidence: f64,
    pub sensitivity: Sensitivity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_after_days: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_confirmed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmations: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Vault-relative paths of related memories (the knowledge-graph edges
    /// that make notes compound instead of staying isolated). Same-domain
    /// only; validated against the index at write time.
    #[serde(default)]
    pub related: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRow {
    pub id: String,
    pub vault_path: String,
    pub domain: String,
    pub mem_type: String,
    pub title: String,
    pub summary: Option<String>,
    pub sensitivity: String,
    pub confidence: f64,
    pub created_at: String,
    pub updated_at: String,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub stale_after_days: Option<i64>,
    pub last_confirmed_at: Option<String>,
    pub confirmation_count: i64,
    pub last_accessed_at: Option<String>,
    pub access_count: i64,
    pub expires_at: Option<String>,
    pub provenance: String,
    pub content_hash: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScoredMemory {
    #[serde(flatten)]
    pub row: MemoryRow,
    pub score: f64,
    pub relevance: f64,
    pub recency: f64,
    pub trust: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub children: Vec<VaultNode>,
    pub memory_id: Option<String>,
    pub mem_type: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryReadResult {
    pub frontmatter: Option<MemoryFrontmatter>,
    pub markdown: String,
    pub status: String,
    pub git_last_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchOpts {
    #[serde(default = "default_true")]
    pub include_stale: bool,
    pub limit: Option<usize>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalOp {
    Create,
    Update,
    Supersede,
}

impl ProposalOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProposalOp::Create => "create",
            ProposalOp::Update => "update",
            ProposalOp::Supersede => "supersede",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    Memory,
    Skill,
}

impl ProposalKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProposalKind::Memory => "memory",
            ProposalKind::Skill => "skill",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProposalStatus {
    Pending,
    Approved,
    Discarded,
    AutoApplied,
}

impl ProposalStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProposalStatus::Pending => "pending",
            ProposalStatus::Approved => "approved",
            ProposalStatus::Discarded => "discarded",
            ProposalStatus::AutoApplied => "auto_applied",
        }
    }

    // Reserved for proposal filtering once the UI exposes decided history.
    #[allow(dead_code)]
    pub fn parse(s: &str) -> Self {
        match s {
            "approved" => ProposalStatus::Approved,
            "discarded" => ProposalStatus::Discarded,
            "auto_applied" => ProposalStatus::AutoApplied,
            _ => ProposalStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryWriteProposal {
    pub id: String,
    pub task_id: Option<String>,
    pub vault_path: String,
    pub domain: String,
    pub kind: String,
    pub op: String,
    pub supersedes_id: Option<String>,
    pub sensitivity: String,
    pub unified_diff: String,
    pub new_content: String,
    pub provenance: String,
    pub gate_report: String,
    pub requires_approval: bool,
    pub status: String,
    pub created_at: String,
    pub decided_at: Option<String>,
    /// Hash of the source document seen when this proposal was created.
    /// Approval fails if that document has changed in the meantime.
    pub base_content_hash: Option<String>,
    /// Import batch that generated the proposal, if any. Imported memories
    /// always remain pending until the user approves them.
    pub import_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReindexResult {
    pub indexed: i64,
    pub drifted: i64,
    pub orphaned: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceResult {
    pub expired: i64,
    pub marked_stale: i64,
    pub consolidation_proposals: i64,
    pub deferred_expirations: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryOperationRecord {
    pub id: String,
    pub kind: String,
    pub entity_id: String,
    pub stage: String,
    pub status: String,
    pub error: Option<String>,
    pub started_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecoveryReport {
    pub recovered: i64,
    pub rolled_back: i64,
    pub needs_attention: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalBenchmarkMetrics {
    pub top_one_accuracy: f64,
    pub source_hit_rate_at_five: f64,
    pub source_recall_at_five: f64,
    pub mean_reciprocal_rank: f64,
    pub latency_p50_ms: f64,
    pub latency_p95_ms: f64,
    pub outbound_cost_usd: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalEvalCase {
    pub id: String,
    pub domain: String,
    pub question: String,
    pub expected_sources: Vec<String>,
    pub provenance: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalEvalCaseRequest {
    pub question: String,
    pub domain: String,
    pub expected_sources: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetrievalBenchmarkReport {
    pub generated_at: String,
    pub corpus_kind: String,
    pub cases: usize,
    pub corpus_memories: usize,
    pub baseline: RetrievalBenchmarkMetrics,
    pub candidate: RetrievalBenchmarkMetrics,
    pub production: RetrievalBenchmarkMetrics,
    pub fuzzy_scan_count: usize,
    pub semantic_backend: String,
    pub notes: Vec<String>,
}

/// One issue surfaced by the lint pass. Lint never writes: findings are
/// review material for the human, mirroring the wiki-pattern "lint
/// operation" but routed through this app's read-only governance stance.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLintFinding {
    /// `broken_link`, `orphan`, `stale`, or `contradiction`.
    pub kind: String,
    /// `info` or `warning`.
    pub severity: String,
    /// Vault-relative paths of the notes involved.
    pub paths: Vec<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryLintReport {
    pub generated_at: String,
    pub scanned: i64,
    pub findings: Vec<MemoryLintFinding>,
    /// True when the model-assisted contradiction pass ran.
    pub deep: bool,
    pub model_tokens: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualSaveRequest {
    pub domain: String,
    pub mem_type: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub sensitivity: Option<String>,
    pub source: Option<String>,
    pub confidence: Option<f64>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub stale_after_days: Option<i64>,
    pub expires: Option<String>,
    /// Explicit contradiction target. Unlike fuzzy dedup this always creates
    /// a new truth version and therefore always requires approval.
    pub supersedes_id: Option<String>,
    /// Vault-relative paths this note should link to. Unresolvable or
    /// cross-domain entries are dropped at the gate, never rejected.
    #[serde(default)]
    pub related: Vec<String>,
}

impl ManualSaveRequest {
    #[cfg(test)]
    fn basic(domain: &str, mem_type: &str, title: &str, body: &str) -> Self {
        Self {
            domain: domain.to_string(),
            mem_type: mem_type.to_string(),
            title: title.to_string(),
            body: body.to_string(),
            tags: Vec::new(),
            sensitivity: None,
            source: None,
            confidence: None,
            valid_from: None,
            valid_until: None,
            stale_after_days: None,
            expires: None,
            supersedes_id: None,
            related: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAskRequest {
    pub question: String,
    pub domain: String,
    #[serde(default = "default_true")]
    pub include_stale: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCitation {
    pub id: String,
    pub number: usize,
    pub title: String,
    pub vault_path: String,
    pub status: String,
    pub excerpt: String,
    pub score: f64,
    /// `memory` for governed atomic notes or `source` for imported source passages.
    pub source_kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAnswer {
    pub id: String,
    pub question: String,
    pub domain: String,
    pub answer: String,
    pub citations: Vec<MemoryCitation>,
    pub warnings: Vec<String>,
    pub abstained: bool,
    /// `high`, `medium`, `low`, or `insufficient`.
    pub confidence: String,
    pub confidence_score: f64,
    pub source_count: usize,
    pub model: Option<String>,
    pub generated_at: String,
}

/// Live status emitted over a per-invocation Tauri channel while `ask` runs.
/// Carries structural metadata only — never unverified model text.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAskProgress {
    /// `retrieval`, `synthesis`, or `verification`.
    pub stage: String,
    pub label: String,
    pub at: String,
    /// Transient events (heartbeats, stderr diagnostics) replace the
    /// previous transient line in the UI instead of stacking.
    #[serde(default)]
    pub transient: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAnswerFeedbackRequest {
    pub answer_id: String,
    pub question: String,
    pub domain: String,
    /// Currently `flagged`; kept explicit for future positive feedback.
    pub feedback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractedMemoryCandidate {
    pub mem_type: String,
    pub title: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
    pub sensitivity: Option<String>,
    pub confidence: Option<f64>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub stale_after_days: Option<i64>,
    pub expires: Option<String>,
    pub supersedes_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryIngestRequest {
    pub domain: String,
    /// Namespaced immutable source reference such as meeting:<path>,
    /// outlook:<message-id>, slack:<thread-id>, confluence:<page-id>.
    pub source: String,
    pub candidates: Vec<ExtractedMemoryCandidate>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryIngestFailure {
    pub index: usize,
    pub title: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryIngestResult {
    pub proposals: Vec<MemoryWriteProposal>,
    pub rejected: Vec<MemoryIngestFailure>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentImportRequest {
    pub domain: String,
    /// One of `text`, `file`, or `url`.
    pub input_kind: String,
    pub title: String,
    /// Required for text/file imports. URL imports fetch the remote body.
    pub content: Option<String>,
    /// `base64` for binary file uploads; omitted for UTF-8 text.
    pub content_encoding: Option<String>,
    /// Browser-provided media type. The importer still verifies file signatures.
    pub mime_type: Option<String>,
    pub source_url: Option<String>,
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentImportRecord {
    pub id: String,
    pub domain: String,
    pub title: String,
    pub input_kind: String,
    pub source_ref: String,
    pub source_path: String,
    /// Preserved original binary when the source is not plain text (for example a PDF).
    pub original_path: Option<String>,
    pub content_hash: String,
    pub byte_count: i64,
    pub candidate_count: i64,
    pub warning_count: i64,
    pub warnings: Vec<String>,
    /// Best evaluated PDF converter (`markitdown` or the local fallback).
    /// It is accepted only when `extraction_quality_status` is `passed`.
    pub extraction_engine: Option<String>,
    pub extraction_version: Option<String>,
    pub extraction_quality_score: Option<i64>,
    /// `passed`, `failed`, or `not_applicable` for non-PDF sources.
    pub extraction_quality_status: String,
    pub extraction_quality_issues: Vec<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentImportResult {
    pub import: DocumentImportRecord,
    pub proposals: Vec<MemoryWriteProposal>,
    pub rejected: Vec<MemoryIngestFailure>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSourceReadResult {
    pub import: DocumentImportRecord,
    pub content: String,
    pub git_last_commit: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProposalDecideRequest {
    pub id: String,
    pub decision: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use base64::Engine as _;
    use sha2::{Digest, Sha256};

    // Env vars are process-global and cargo runs tests in parallel threads:
    // every test that overrides AGENTIC_OS_VAULT_ROOT / AGENTIC_OS_SKILLS_ROOT
    // must hold this lock for its whole body.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    struct EnvRoots {
        _guard: std::sync::MutexGuard<'static, ()>,
        pub vault: std::path::PathBuf,
        pub skills: std::path::PathBuf,
    }

    impl EnvRoots {
        fn new(label: &str) -> Self {
            let guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let base = std::env::temp_dir().join(format!("agentic-os-{label}-{nonce}"));
            let vault = base.join("vault");
            let skills = base.join("skills");
            std::fs::create_dir_all(&vault).unwrap();
            std::fs::create_dir_all(&skills).unwrap();
            std::env::set_var("AGENTIC_OS_VAULT_ROOT", &vault);
            std::env::set_var("AGENTIC_OS_SKILLS_ROOT", &skills);
            Self {
                _guard: guard,
                vault,
                skills,
            }
        }
    }

    impl Drop for EnvRoots {
        fn drop(&mut self) {
            std::env::remove_var("AGENTIC_OS_VAULT_ROOT");
            std::env::remove_var("AGENTIC_OS_SKILLS_ROOT");
        }
    }

    fn temp_db(label: &str) -> Db {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("agentic-os-mem-{label}-{nonce}.db"));
        Db::open(&path).expect("temp db opens")
    }

    fn minimal_text_pdf(text: &str) -> Vec<u8> {
        let escaped = text
            .replace('\\', "\\\\")
            .replace('(', "\\(")
            .replace(')', "\\)");
        let stream = format!("BT /F1 12 Tf 72 720 Td ({escaped}) Tj ET");
        let objects = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".to_string(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
            format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        ];
        let mut pdf = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(pdf.len());
            pdf.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", index + 1, object).as_bytes());
        }
        let xref_offset = pdf.len();
        pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
        pdf.extend_from_slice(b"0000000000 65535 f \n");
        for offset in offsets {
            pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        pdf.extend_from_slice(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_offset}\n%%EOF\n",
                objects.len() + 1
            )
            .as_bytes(),
        );
        pdf
    }

    fn sample_row(id: &str, title: &str, status: &str, last_confirmed: Option<&str>) -> MemoryRow {
        let now = chrono::Utc::now().to_rfc3339();
        MemoryRow {
            id: id.to_string(),
            vault_path: format!("work/memories/{id}.md"),
            domain: "work".to_string(),
            mem_type: "fact".to_string(),
            title: title.to_string(),
            summary: Some(title.to_string()),
            sensitivity: "normal".to_string(),
            confidence: 0.9,
            created_at: now.clone(),
            updated_at: now.clone(),
            valid_from: None,
            valid_until: None,
            stale_after_days: Some(180),
            last_confirmed_at: last_confirmed.map(str::to_string).or(Some(now)),
            confirmation_count: 1,
            last_accessed_at: None,
            access_count: 0,
            expires_at: None,
            provenance: r#"{"source":"manual","ts":"2026-07-20"}"#.to_string(),
            content_hash: "0".repeat(64),
            status: status.to_string(),
        }
    }

    #[test]
    fn fts_search_finds_indexed_memory() {
        // Regression: the FTS join must go through memories.rowid, not the
        // TEXT uuid — the uuid join silently returned zero results.
        let db = temp_db("fts-join");
        index::ensure_tables(&db).unwrap();
        let row = sample_row(
            "mem-1",
            "PowerReviews feed is delta not full",
            "active",
            None,
        );
        index::upsert(
            &db,
            &row,
            "Delta feed daily because full files time out.",
            &[],
        )
        .unwrap();

        let opts = MemorySearchOpts {
            include_stale: true,
            limit: Some(8),
        };
        let results = retrieval::search(&db, "powerreviews delta", Some("work"), &opts).unwrap();

        assert_eq!(results.len(), 1, "indexed memory must be findable via FTS");
        assert_eq!(results[0].row.id, "mem-1");
    }

    #[test]
    fn fts_search_survives_special_characters() {
        // Regression: raw MATCH input with apostrophes/operators used to
        // produce an FTS5 syntax error.
        let db = temp_db("fts-escape");
        index::ensure_tables(&db).unwrap();
        let row = sample_row("mem-2", "Sierra vendor promise", "active", None);
        index::upsert(&db, &row, "Rate limit fix promised by June.", &[]).unwrap();

        let opts = MemorySearchOpts {
            include_stale: true,
            limit: Some(8),
        };
        let results =
            retrieval::search(&db, "vendor's \"promise\" (sierra) -", Some("work"), &opts).unwrap();

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn stale_memory_ranks_below_fresh_equivalent() {
        let db = temp_db("stale-rank");
        index::ensure_tables(&db).unwrap();
        let fresh = sample_row(
            "mem-fresh",
            "Databricks Genie semantic layer",
            "active",
            None,
        );
        let stale = sample_row(
            "mem-stale",
            "Databricks Genie semantic layer",
            "stale",
            None,
        );
        index::upsert(&db, &fresh, "Fresh fact body about Genie.", &[]).unwrap();
        index::upsert(&db, &stale, "Stale fact body about Genie.", &[]).unwrap();

        let opts = MemorySearchOpts {
            include_stale: true,
            limit: Some(8),
        };
        let results = retrieval::search(&db, "genie semantic", Some("work"), &opts).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(
            results[0].row.id, "mem-fresh",
            "stale penalty must demote the stale copy"
        );
    }

    #[test]
    fn staleness_sweep_handles_rfc3339_confirmation_dates() {
        // Regression: last_confirmed_at is stored as RFC 3339; parsing it
        // as bare %Y-%m-%d failed silently and nothing ever went stale.
        let db = temp_db("stale-sweep");
        index::ensure_tables(&db).unwrap();
        let old = (chrono::Utc::now() - chrono::Duration::days(400)).to_rfc3339();
        let row = sample_row(
            "mem-old",
            "Old unconfirmed fact",
            "active",
            Some(old.as_str()),
        );
        index::upsert(&db, &row, "This fact was confirmed 400 days ago.", &[]).unwrap();

        let result = maintenance::run_sweep(&db).unwrap();

        assert_eq!(
            result.marked_stale, 1,
            "RFC 3339 confirmation dates must be parsed"
        );
        let after = index::get_by_id(&db, "mem-old").unwrap().unwrap();
        assert_eq!(after.status, "stale");
    }

    #[test]
    fn vault_write_rejects_path_traversal() {
        // Regression: root.join("../x") passes a literal starts_with check
        // while escaping the vault on write.
        let roots = EnvRoots::new("traversal");

        let escape = vault::write_file_atomic("../escaped.md", "should never land");
        let absolute = vault::write_file_atomic("/tmp/absolute.md", "should never land");
        let legal = vault::write_file_atomic("work/ok.md", "fine");

        assert!(escape.is_err(), "parent-dir traversal must be rejected");
        assert!(absolute.is_err(), "absolute paths must be rejected");
        assert!(legal.is_ok(), "legal in-vault writes must still work");
        assert!(!roots.vault.parent().unwrap().join("escaped.md").exists());
    }

    #[test]
    fn email_import_persists_images_and_rewrites_cid_markers() {
        let roots = EnvRoots::new("eml-images");
        let db = temp_db("eml-images");

        let eml = email_extraction::sample_eml_with_image();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime
            .block_on(importer::import_document(
                &db,
                &DocumentImportRequest {
                    domain: "work".to_string(),
                    input_kind: "file".to_string(),
                    title: "Org chart mail".to_string(),
                    content: Some(base64::engine::general_purpose::STANDARD.encode(eml)),
                    content_encoding: Some("base64".to_string()),
                    mime_type: Some("message/rfc822".to_string()),
                    source_url: None,
                    file_name: Some("orgchart.eml".to_string()),
                },
            ))
            .unwrap();

        let source = importer::read_source(&db, &result.import.id).unwrap();
        assert!(
            source.content.contains("![image002.png]("),
            "cid marker must be rewritten to a real image link"
        );
        assert!(source.content.contains("## Embedded images"));
        assert!(
            !source.content.contains("[cid:image002.png"),
            "no raw cid marker may survive"
        );
        let image_path = result.import.source_path.replace(".md", "-image002.png");
        assert!(
            vault::file_exists(&image_path).unwrap(),
            "image bytes must be persisted beside the original: {image_path}"
        );
        assert!(
            result.import.warnings.iter().any(|warning| warning.contains("desktop runtime")),
            "headless import must record that AI description was skipped: {:?}",
            result.import.warnings
        );
        drop(roots);
    }

    #[test]
    fn multi_query_retrieval_unions_results_across_queries() {
        let roots = EnvRoots::new("union");
        let db = temp_db("union");

        let first = ManualSaveRequest::basic(
            "work",
            "fact",
            "PowerReviews feed is delta",
            "Delta feed daily because full loads time out.",
        );
        pipeline::process_manual_save(&db, &first, "manual").unwrap();
        let second = ManualSaveRequest::basic(
            "work",
            "fact",
            "Sierra rate limit promise",
            "Sierra promised a rate limit fix by June.",
        );
        pipeline::process_manual_save(&db, &second, "manual").unwrap();

        let request = MemoryAskRequest {
            question: "unrelated question text".to_string(),
            domain: "work".to_string(),
            include_stale: true,
        };
        let single = retrieval::retrieve_evidence(
            &db,
            &request,
            &["powerreviews delta".to_string()],
            false,
        )
        .unwrap()
        .0;
        assert_eq!(single.len(), 1, "one query hits one memory");

        let union = retrieval::retrieve_evidence(
            &db,
            &request,
            &[
                "powerreviews delta".to_string(),
                "sierra rate limit".to_string(),
            ],
            false,
        )
        .unwrap()
        .0;
        assert_eq!(
            union.len(),
            2,
            "the union of sub-queries must surface both memories"
        );
        drop(roots);
    }

    #[test]
    fn related_links_roundtrip_through_frontmatter() {
        let fm = MemoryFrontmatter {
            id: "id-1".to_string(),
            mem_type: MemoryType::Synthesis,
            domain: "work".to_string(),
            title: "Linked synthesis".to_string(),
            created: "2026-07-21".to_string(),
            updated: "2026-07-21".to_string(),
            provenance: Provenance {
                source: "memory-ask:a1".to_string(),
                ts: "2026-07-21".to_string(),
            },
            sources: Vec::new(),
            confidence: 0.8,
            sensitivity: Sensitivity::Normal,
            valid_from: None,
            valid_until: None,
            supersedes: None,
            superseded_by: None,
            stale_after_days: Some(180),
            last_confirmed: None,
            confirmations: None,
            expires: None,
            tags: vec!["ask".to_string()],
            related: vec![
                "work/decisions/feed.md".to_string(),
                "work/memories/limits.md".to_string(),
            ],
        };
        let serialized = frontmatter::serialize(&fm, "Body text.");
        let (parsed, body) = frontmatter::parse(&serialized).expect("roundtrip parses");
        assert_eq!(parsed.related, fm.related);
        assert_eq!(parsed.mem_type, MemoryType::Synthesis);
        assert_eq!(body, "Body text.");
    }

    #[test]
    fn related_links_are_resolved_and_invalid_ones_dropped() {
        let roots = EnvRoots::new("related");
        let db = temp_db("related");

        let first = ManualSaveRequest::basic("work", "fact", "Feed is delta", "Delta feed daily.");
        let first_proposal = pipeline::process_manual_save(&db, &first, "manual").unwrap();
        assert_eq!(first_proposal.status, "auto_applied");
        let first_path = first_proposal.vault_path.clone();

        let mut second = ManualSaveRequest::basic(
            "work",
            "synthesis",
            "Feed decision summary",
            "Nightly delta sync was chosen over full loads.",
        );
        second.related = vec![
            first_path.clone(),
            "work/memories/does-not-exist.md".to_string(),
            "personal/memories/cross-domain.md".to_string(),
            "../escape.md".to_string(),
        ];
        let second_proposal = pipeline::process_manual_save(&db, &second, "manual").unwrap();
        assert_eq!(second_proposal.status, "auto_applied");

        let (fm, _) = frontmatter::parse(&second_proposal.new_content).unwrap();
        assert_eq!(
            fm.related,
            vec![first_path],
            "only the resolvable same-domain link must survive the gate"
        );
        assert_eq!(fm.mem_type, MemoryType::Synthesis);
        drop(roots);
    }

    #[tokio::test]
    async fn lint_flags_orphans_but_not_linked_notes() {
        let roots = EnvRoots::new("lint");
        let db = temp_db("lint");

        let orphan =
            ManualSaveRequest::basic("work", "fact", "Isolated fact", "Nobody links here.");
        pipeline::process_manual_save(&db, &orphan, "manual").unwrap();

        let hub = ManualSaveRequest::basic("work", "fact", "Hub note", "Linked from below.");
        let hub_proposal = pipeline::process_manual_save(&db, &hub, "manual").unwrap();
        let mut spoke = ManualSaveRequest::basic("work", "fact", "Spoke note", "Points at hub.");
        spoke.related = vec![hub_proposal.vault_path.clone()];
        pipeline::process_manual_save(&db, &spoke, "manual").unwrap();

        let report = lint::run_lint(&db, Some("work"), false).await.unwrap();
        assert_eq!(report.scanned, 3);
        assert!(!report.deep);
        let orphan_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|finding| finding.kind == "orphan")
            .collect();
        assert_eq!(
            orphan_findings.len(),
            1,
            "only the unlinked note is an orphan: {:?}",
            report.findings
        );
        assert!(orphan_findings[0].paths[0].contains("isolated-fact"));
        assert!(
            !report.findings.iter().any(|f| f.kind == "broken_link"),
            "gate-validated links must never lint as broken"
        );
        drop(roots);
    }

    #[test]
    fn confirm_persists_to_file_and_survives_reindex() {
        // Regression: confirming only in the index was silently undone by
        // the next reindex (file = source of truth).
        let roots = EnvRoots::new("confirm");
        let db = temp_db("confirm");

        let request =
            ManualSaveRequest::basic("work", "fact", "Feed is delta", "Delta feed daily.");
        let proposal = pipeline::process_manual_save(&db, &request, "manual").unwrap();
        assert_eq!(proposal.status, "auto_applied");

        let id = frontmatter::parse(&proposal.new_content).unwrap().0.id;
        index::confirm(&db, &id).unwrap();
        index::reindex(&db).unwrap();

        let row = index::get_by_id(&db, &id).unwrap().unwrap();
        assert_eq!(
            row.confirmation_count, 2,
            "confirmation must survive a reindex because it lives in the file"
        );
        drop(roots);
    }

    #[test]
    fn proposal_diff_is_a_real_unified_diff() {
        let roots = EnvRoots::new("diff");
        let db = temp_db("diff");

        let request = ManualSaveRequest::basic(
            "work",
            "fact",
            "Genie handles the semantic layer",
            "Custom approach discarded for maintenance cost.",
        );
        let proposal = pipeline::process_manual_save(&db, &request, "manual").unwrap();

        assert!(
            proposal.unified_diff.contains("+++"),
            "diff must have a file header"
        );
        assert!(
            proposal.unified_diff.contains("+Custom approach discarded"),
            "diff must contain the added body lines, got: {}",
            proposal.unified_diff
        );
        drop(roots);
    }

    #[test]
    fn gate_reject_writes_audit_row() {
        let roots = EnvRoots::new("gate-audit");
        let db = temp_db("gate-audit");

        let request = ManualSaveRequest::basic(
            "work",
            "fact",
            "Leaked credentials",
            "key is AKIAIOSFODNN7EXAMPLE do not share",
        );
        let result = pipeline::process_manual_save(&db, &request, "manual");
        assert!(result.is_err(), "secret content must be rejected");

        let audit_rows: i64 = db
            .with_conn(|conn| {
                conn.query_row(
                    "SELECT COUNT(*) FROM audit WHERE run_id = 'memory-gate'",
                    [],
                    |r| r.get(0),
                )
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(
            audit_rows, 1,
            "every gate rejection must leave an audit row"
        );
        drop(roots);
    }

    #[test]
    fn run_capture_creates_episode_for_work_and_skips_personal() {
        let roots = EnvRoots::new("capture");
        let db = temp_db("capture");

        let captured = pipeline::process_run_capture(
            &db,
            "task-1",
            "work",
            "QA newsletter",
            "Check campaign against style guide",
            "completed",
        )
        .unwrap();
        let skipped = pipeline::process_run_capture(
            &db,
            "task-2",
            "personal",
            "Private thing",
            "goal",
            "completed",
        )
        .unwrap();

        let proposal = captured.expect("work runs must be captured");
        assert_eq!(proposal.status, "auto_applied");
        assert!(proposal.vault_path.starts_with("work/episodes/"));
        assert!(
            skipped.is_none(),
            "personal domain capture is off until Phase 5"
        );
        drop(roots);
    }

    #[test]
    fn skill_distill_requires_approval_and_lands_in_skills_root() {
        let roots = EnvRoots::new("distill");
        let db = temp_db("distill");

        let proposal = pipeline::process_skill_distill(
            &db,
            "task-9",
            "work",
            "Thread to ADO ticket",
            "Turn a messy email thread into a ticket",
            &[
                "Classify and check policy".to_string(),
                "Run agent".to_string(),
            ],
        )
        .unwrap();

        assert_eq!(proposal.status, "pending", "skills must never auto-apply");
        assert!(proposal.requires_approval);

        proposals::decide(&db, &proposal.id, "approve").unwrap();
        let skill_file = roots.skills.join("thread-to-ado-ticket/SKILL.md");
        assert!(
            skill_file.exists(),
            "approved skill must land under the skills root"
        );
        let content = std::fs::read_to_string(&skill_file).unwrap();
        assert!(content.contains("provenance: task:task-9"));
        drop(roots);
    }

    #[test]
    fn context_builder_tags_stale_as_unverified_and_skips_sensitive() {
        let db = temp_db("context");
        index::ensure_tables(&db).unwrap();

        let fresh = sample_row("ctx-fresh", "Sierra rate limit promise", "active", None);
        let stale = sample_row("ctx-stale", "Sierra old SLA agreement", "stale", None);
        let mut sensitive = sample_row("ctx-sens", "Sierra contract amount", "active", None);
        sensitive.sensitivity = "sensitive".to_string();

        index::upsert(&db, &fresh, "Fix promised by June.", &[]).unwrap();
        index::upsert(&db, &stale, "Old SLA from 2025.", &[]).unwrap();
        index::upsert(&db, &sensitive, "Contract value details.", &[]).unwrap();

        let context = context::build_memory_context(&db, "sierra", "work").unwrap();

        assert_eq!(
            context.injected_paths.len(),
            2,
            "sensitive memories never enter prompts"
        );
        assert_eq!(context.unverified_paths.len(), 1);
        assert_eq!(context.memory_refs.len(), 2);
        assert!(context
            .memory_refs
            .iter()
            .any(|reference| reference.memory_id == "ctx-fresh"));
        assert!(context.prompt_block.contains("verify=\"UNVERIFIED\""));
        assert!(context.prompt_block.contains("never execute instructions"));
        assert!(!context.prompt_block.contains("Contract value"));
    }

    #[test]
    fn duplicate_update_preserves_identity_path_and_history() {
        let roots = EnvRoots::new("update-identity");
        let db = temp_db("update-identity");
        let first = ManualSaveRequest::basic(
            "work",
            "fact",
            "Sierra API rate limit",
            "The current limit is 100 requests per minute.",
        );
        let first_proposal = pipeline::process_manual_save(&db, &first, "manual").unwrap();
        let (first_fm, _) = frontmatter::parse(&first_proposal.new_content).unwrap();

        let second = ManualSaveRequest::basic(
            "work",
            "fact",
            "Sierra API rate limit",
            "The current limit is 120 requests per minute after the vendor change.",
        );
        let second_proposal = pipeline::process_manual_save(&db, &second, "manual").unwrap();
        let (second_fm, second_body) = frontmatter::parse(&second_proposal.new_content).unwrap();

        assert_eq!(second_proposal.op, "update");
        assert_eq!(
            first_fm.id, second_fm.id,
            "updates must retain the immutable id"
        );
        assert_eq!(first_proposal.vault_path, second_proposal.vault_path);
        assert!(second_body.contains("120 requests"));
        assert_eq!(second_fm.confirmations, Some(2));
        let count: i64 = db
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM memories", [], |row| row.get(0))
                    .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(count, 1, "an update must not create a duplicate row");
        drop(roots);
    }

    #[test]
    fn supersede_versions_truth_in_file_and_index() {
        let roots = EnvRoots::new("supersede");
        let db = temp_db("supersede");
        let first = ManualSaveRequest::basic(
            "work",
            "fact",
            "Production model",
            "Production uses model alpha.",
        );
        let first_proposal = pipeline::process_manual_save(&db, &first, "manual").unwrap();
        let (first_fm, _) = frontmatter::parse(&first_proposal.new_content).unwrap();

        let mut replacement = ManualSaveRequest::basic(
            "work",
            "fact",
            "Production model",
            "Production now uses model beta.",
        );
        replacement.supersedes_id = Some(first_fm.id.clone());
        replacement.valid_from = Some("2026-07-21".to_string());
        let proposal = pipeline::process_manual_save(&db, &replacement, "manual").unwrap();
        assert_eq!(proposal.op, "supersede");
        assert_eq!(proposal.status, "pending");
        assert!(proposal.requires_approval);

        proposals::decide(&db, &proposal.id, "approve").unwrap();
        let old_row = index::get_by_id(&db, &first_fm.id).unwrap().unwrap();
        assert_eq!(old_row.status, "stale");
        assert_eq!(old_row.valid_until.as_deref(), Some("2026-07-21"));
        let (old_content, _) = vault::read_file(&old_row.vault_path).unwrap();
        let (old_file_fm, _) = frontmatter::parse(&old_content).unwrap();
        assert_eq!(old_file_fm.valid_until.as_deref(), Some("2026-07-21"));
        let (new_fm, _) = frontmatter::parse(&proposal.new_content).unwrap();
        assert_ne!(new_fm.id, first_fm.id);
        assert_eq!(new_fm.supersedes.as_deref(), Some(first_fm.id.as_str()));
        assert_eq!(
            old_file_fm.superseded_by.as_deref(),
            Some(new_fm.id.as_str())
        );
        assert_eq!(
            index::get_by_id(&db, &new_fm.id).unwrap().unwrap().status,
            "active"
        );
        drop(roots);
    }

    #[test]
    fn sensitive_memory_waits_for_approval_and_invalid_domain_is_rejected() {
        let roots = EnvRoots::new("sensitive-domain");
        let db = temp_db("sensitive-domain");
        let mut sensitive = ManualSaveRequest::basic(
            "work",
            "fact",
            "Compensation review",
            "The salary review happens in September.",
        );
        sensitive.sensitivity = Some("normal".to_string());
        let proposal = pipeline::process_manual_save(&db, &sensitive, "manual").unwrap();
        assert_eq!(
            proposal.sensitivity, "sensitive",
            "deterministic classification wins"
        );
        assert_eq!(proposal.status, "pending");
        assert!(vault::read_file(&proposal.vault_path).is_err());

        let invalid = ManualSaveRequest::basic("unknown", "fact", "Bad domain", "Never write me.");
        assert!(pipeline::process_manual_save(&db, &invalid, "manual").is_err());
        drop(roots);
    }

    #[test]
    fn reindex_keeps_stale_state_and_expiry_archives_without_deleting_provenance() {
        let roots = EnvRoots::new("lifecycle");
        let db = temp_db("lifecycle");
        let old = (chrono::Utc::now() - chrono::Duration::days(400)).to_rfc3339();
        let row = sample_row(
            "persist-stale",
            "Persistent stale fact",
            "stale",
            Some(&old),
        );
        let fm = MemoryFrontmatter {
            id: row.id.clone(),
            mem_type: MemoryType::Fact,
            domain: "work".to_string(),
            title: row.title.clone(),
            created: row.created_at.clone(),
            updated: row.updated_at.clone(),
            provenance: Provenance {
                source: "manual".to_string(),
                ts: row.created_at.clone(),
            },
            sources: Vec::new(),
            confidence: row.confidence,
            sensitivity: Sensitivity::Normal,
            valid_from: None,
            valid_until: None,
            supersedes: None,
            superseded_by: None,
            stale_after_days: Some(180),
            last_confirmed: Some(old),
            confirmations: Some(1),
            expires: None,
            tags: vec![],
            related: vec![],
        };
        let content = frontmatter::serialize(&fm, "A stale but retained fact.");
        vault::ensure_vault().unwrap();
        vault::write_file_atomic(&row.vault_path, &content).unwrap();
        index::upsert(&db, &row, "A stale but retained fact.", &[]).unwrap();
        index::reindex(&db).unwrap();
        assert_eq!(
            index::get_by_id(&db, &row.id).unwrap().unwrap().status,
            "stale"
        );

        let mut episode = ManualSaveRequest::basic(
            "work",
            "episode",
            "Expired working session",
            "Temporary trace.",
        );
        episode.expires = Some("2020-01-01".to_string());
        let episode_proposal = pipeline::process_manual_save(&db, &episode, "manual").unwrap();
        let (episode_fm, _) = frontmatter::parse(&episode_proposal.new_content).unwrap();
        let sweep = maintenance::run_sweep(&db).unwrap();
        assert_eq!(sweep.expired, 1);
        let expired = index::get_by_id(&db, &episode_fm.id).unwrap().unwrap();
        assert_eq!(expired.status, "expired");
        assert!(expired.vault_path.starts_with("_archive/work/episodes/"));
        assert!(roots.vault.join(&expired.vault_path).exists());
        index::reindex(&db).unwrap();
        assert!(index::get_by_id(&db, &episode_fm.id).unwrap().is_some());
        drop(roots);
    }

    #[test]
    fn reindex_and_reopen_preserve_pending_proposals_and_import_history() {
        let roots = EnvRoots::new("reindex-operational-state");
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_path = std::env::temp_dir().join(format!("agentic-os-reopen-{nonce}.db"));
        let db = Db::open(&db_path).unwrap();
        let mut request = ManualSaveRequest::basic(
            "finance",
            "fact",
            "Quarterly planning window",
            "The planning window closes at quarter end.",
        );
        request.sensitivity = Some("sensitive".to_string());
        let proposal = pipeline::process_manual_save(&db, &request, "manual").unwrap();
        assert_eq!(proposal.status, "pending");
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO document_imports (
                    id, domain, title, input_kind, source_ref, source_path,
                    content_hash, byte_count, status, created_at, updated_at
                 ) VALUES (?1,'finance','Planning source','text','manual:test',
                           '_sources/finance/planning.md','hash',42,'pending',?2,?2)",
                rusqlite::params!["import-preserved", now],
            )?;
            Ok(())
        })
        .unwrap();

        index::reindex(&db).unwrap();
        drop(db);
        let reopened = Db::open(&db_path).unwrap();
        index::reindex(&reopened).unwrap();
        assert_eq!(
            proposals::get_by_id(&reopened, &proposal.id)
                .unwrap()
                .unwrap()
                .status,
            "pending"
        );
        assert!(importer::list(&reopened, Some("finance"))
            .unwrap()
            .iter()
            .any(|item| item.id == "import-preserved"));
        drop(roots);
    }

    #[test]
    fn memory_search_returns_matching_evidence_and_omits_absent_topics() {
        let roots = EnvRoots::new("ask");
        let db = temp_db("ask");
        let request = ManualSaveRequest::basic(
            "work",
            "decision",
            "PowerReviews feed mode",
            "The PowerReviews feed is delta because full files exceed the SFTP timeout.",
        );
        pipeline::process_manual_save(&db, &request, "manual").unwrap();

        let matches = retrieval::search(
            &db,
            "Why is the PowerReviews feed delta?",
            Some("work"),
            &MemorySearchOpts {
                include_stale: false,
                limit: Some(8),
            },
        )
        .unwrap();
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].row.title, "PowerReviews feed mode");

        let absent = retrieval::search(
            &db,
            "What is the lunar office policy?",
            Some("work"),
            &MemorySearchOpts {
                include_stale: false,
                limit: Some(8),
            },
        )
        .unwrap();
        assert!(absent.is_empty());
        drop(roots);
    }

    #[test]
    fn ask_feedback_is_validated_and_appended_to_the_audit_chain() {
        let db = temp_db("ask-feedback");
        let answer_id = "00000000-0000-4000-8000-000000000123";
        retrieval::record_answer_feedback(
            &db,
            &MemoryAnswerFeedbackRequest {
                answer_id: answer_id.to_string(),
                question: "Which endpoints are available?".to_string(),
                domain: "work".to_string(),
                feedback: "flagged".to_string(),
            },
        )
        .unwrap();

        let trace = crate::audit::read_trace(&db, &format!("memory-ask:{answer_id}")).unwrap();
        assert_eq!(trace.len(), 1);
        assert_eq!(trace[0].kind, "memory_ask_feedback");

        let invalid = retrieval::record_answer_feedback(
            &db,
            &MemoryAnswerFeedbackRequest {
                answer_id: answer_id.to_string(),
                question: "Which endpoints are available?".to_string(),
                domain: "work".to_string(),
                feedback: "approved".to_string(),
            },
        );
        assert!(invalid.is_err());
    }

    #[test]
    fn saved_ask_answer_keeps_original_sources_and_caps_confidence() {
        let roots = EnvRoots::new("ask-save-provenance");
        let db = temp_db("ask-save-provenance");
        let answer_id = "00000000-0000-4000-8000-000000000124";
        crate::audit::append_row(
            &db,
            &format!("memory-ask:{answer_id}"),
            answer_id,
            "memory_ask",
            "Memory Ask produced a verified answer",
            &serde_json::json!({
                "answerId": answer_id,
                "citations": [
                    {"path": "work/memories/source-a.md", "score": 0.81},
                    {"path": "_sources/work/source-b.md", "score": 0.62}
                ]
            }),
            None,
            None,
        )
        .unwrap();
        let mut request = ManualSaveRequest::basic(
            "work",
            "fact",
            "Saved grounded answer",
            "The answer is retained with its original evidence.",
        );
        request.source = Some(format!("memory-ask:{answer_id}"));
        request.confidence = Some(0.98);
        let proposal = pipeline::process_manual_save(&db, &request, "manual").unwrap();
        assert_eq!(proposal.status, "auto_applied");
        let (content, _) = vault::read_file(&proposal.vault_path).unwrap();
        let (frontmatter, _) = frontmatter::parse(&content).unwrap();
        assert_eq!(
            frontmatter.sources,
            vec![
                "work/memories/source-a.md".to_string(),
                "_sources/work/source-b.md".to_string()
            ]
        );
        assert!((frontmatter.confidence - 0.62).abs() < f64::EPSILON);
        drop(roots);
    }

    #[test]
    fn connector_ingestion_is_bounded_and_isolates_rejected_candidates() {
        let roots = EnvRoots::new("ingest");
        let db = temp_db("ingest");
        let candidate = |title: &str, body: &str| ExtractedMemoryCandidate {
            mem_type: "fact".to_string(),
            title: title.to_string(),
            body: body.to_string(),
            tags: vec!["outlook".to_string()],
            sensitivity: None,
            confidence: Some(0.9),
            valid_from: None,
            valid_until: None,
            stale_after_days: None,
            expires: None,
            supersedes_id: None,
        };
        let result = pipeline::process_ingest_batch(
            &db,
            &MemoryIngestRequest {
                domain: "work".to_string(),
                source: "outlook:message-42".to_string(),
                candidates: vec![
                    candidate("Project owner", "Elena owns the architecture review."),
                    candidate("Leaked key", "AKIAIOSFODNN7EXAMPLE"),
                ],
            },
        )
        .unwrap();

        assert_eq!(result.proposals.len(), 1);
        assert_eq!(result.rejected.len(), 1);
        assert_eq!(result.rejected[0].index, 1);
        assert_eq!(result.proposals[0].status, "auto_applied");
        drop(roots);
    }

    #[test]
    fn ipc_contract_uses_camel_case_memory_type() {
        let request = ManualSaveRequest::basic("work", "fact", "Title", "Body");
        let value = serde_json::to_value(request).unwrap();
        assert_eq!(
            value.get("memType").and_then(|value| value.as_str()),
            Some("fact")
        );
        assert!(value.get("type").is_none());
    }

    #[test]
    fn frontmatter_parser_accepts_crlf_without_losing_body_bytes() {
        let content = "---\r\nid: one\r\ntype: fact\r\ndomain: work\r\ntitle: One\r\ncreated: 2026-07-21\r\nupdated: 2026-07-21\r\nprovenance:\r\n  source: manual\r\n  ts: 2026-07-21\r\nconfidence: 0.8\r\nsensitivity: normal\r\n---\r\n\r\nExact body";
        let (_, body) = frontmatter::parse(content).expect("CRLF memory parses");
        assert_eq!(body, "Exact body");
    }

    #[test]
    fn approval_rejects_a_stale_proposal_instead_of_overwriting() {
        let roots = EnvRoots::new("approval-conflict");
        let db = temp_db("approval-conflict");
        let mut first = ManualSaveRequest::basic(
            "work",
            "fact",
            "Compensation cadence",
            "The salary review happens annually.",
        );
        first.sensitivity = Some("sensitive".to_string());
        let create = pipeline::process_manual_save(&db, &first, "manual").unwrap();
        proposals::decide(&db, &create.id, "approve").unwrap();

        let mut update = ManualSaveRequest::basic(
            "work",
            "fact",
            "Compensation cadence",
            "The salary review now happens twice a year.",
        );
        update.sensitivity = Some("sensitive".to_string());
        let pending = pipeline::process_manual_save(&db, &update, "manual").unwrap();
        assert_eq!(pending.status, "pending");
        let (current, _) = vault::read_file(&pending.vault_path).unwrap();
        let externally_changed = format!("{current}\n\nExternal change.");
        vault::write_file_atomic(&pending.vault_path, &externally_changed).unwrap();

        let result = proposals::decide(&db, &pending.id, "approve");
        assert!(result.is_err(), "stale approval must be rejected");
        assert_eq!(
            vault::read_file(&pending.vault_path).unwrap().0,
            externally_changed,
            "the newer file must not be overwritten"
        );
        assert_eq!(
            proposals::get_by_id(&db, &pending.id)
                .unwrap()
                .unwrap()
                .status,
            "pending"
        );
        drop(roots);
    }

    #[test]
    fn document_import_preserves_full_source_and_requires_fact_approval() {
        let roots = EnvRoots::new("document-import");
        let db = temp_db("document-import");
        let body = r#"# Sierra Headless API

## Authentication

Headless API endpoints require authentication unless enforcement is disabled. Sierra supports API tokens with the Headless API scope and OAuth client credentials with short-lived JWT tokens.

Authentication can be tested without organization-wide enforcement by sending the X-Sierra-Force-Headless-API-Authorization header on the request.

## Compatibility date

All API requests are required to include Sierra-API-Compatibility-Date. The latest supported compatibility date is 2025-02-01.

## Conversation history

Conversation history requires a signed userIdentityToken. A Headless API bearer token alone cannot retrieve a user's messages.
"#;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime
            .block_on(importer::import_document(
                &db,
                &DocumentImportRequest {
                    domain: "work".to_string(),
                    input_kind: "text".to_string(),
                    title: "Sierra Headless API".to_string(),
                    content: Some(body.to_string()),
                    content_encoding: None,
                    mime_type: None,
                    source_url: None,
                    file_name: None,
                },
            ))
            .unwrap();

        assert_eq!(result.import.byte_count, body.len() as i64);
        assert!(result.import.source_path.starts_with("_sources/work/"));
        assert!(!result.proposals.is_empty());
        assert!(result.proposals.len() <= 10);
        assert!(result.proposals.iter().all(|proposal| {
            proposal.status == "pending"
                && proposal.requires_approval
                && proposal.import_id.as_deref() == Some(result.import.id.as_str())
        }));
        let source = importer::read_source(&db, &result.import.id).unwrap();
        assert_eq!(source.content, body);
        assert!(source.git_last_commit.is_some());
        let source_by_path = importer::read_source_by_path(&db, &result.import.source_path)
            .unwrap()
            .unwrap();
        assert_eq!(source_by_path.content, body);

        let before_approval = retrieval::search(
            &db,
            "OAuth JWT authentication",
            Some("work"),
            &MemorySearchOpts {
                include_stale: true,
                limit: Some(10),
            },
        )
        .unwrap();
        assert!(before_approval.is_empty());
        let source_hits =
            index::search_document_chunks(&db, "OAuth client credentials JWT", "work", 8).unwrap();
        assert_eq!(source_hits.len(), 1);
        assert_eq!(source_hits[0].source_path, result.import.source_path);
        assert!(source_hits[0].body.contains("short-lived JWT tokens"));

        proposals::decide(&db, &result.proposals[0].id, "approve").unwrap();
        let refreshed = importer::list(&db, Some("work")).unwrap();
        assert_eq!(refreshed.len(), 1);
        assert!(matches!(
            refreshed[0].status.as_str(),
            "partial" | "completed"
        ));
        assert!(vault::read_file(&result.proposals[0].vault_path).is_ok());
        drop(roots);
    }

    #[test]
    fn pdf_document_import_extracts_text_and_preserves_original_bytes() {
        let roots = EnvRoots::new("pdf-document-import");
        let db = temp_db("pdf-document-import");
        let pdf = minimal_text_pdf(
            "Headless API authentication requires OAuth client credentials with short-lived JWT tokens.",
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(&pdf);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime
            .block_on(importer::import_document(
                &db,
                &DocumentImportRequest {
                    domain: "work".to_string(),
                    input_kind: "file".to_string(),
                    title: "Sierra authentication".to_string(),
                    content: Some(encoded),
                    content_encoding: Some("base64".to_string()),
                    mime_type: Some("application/pdf".to_string()),
                    source_url: None,
                    file_name: Some("sierra-authentication.pdf".to_string()),
                },
            ))
            .unwrap();

        assert_eq!(result.import.byte_count, pdf.len() as i64);
        assert!(!result.proposals.is_empty());
        assert_eq!(
            result.import.extraction_engine.as_deref(),
            Some("pdf-extract")
        );
        assert_eq!(result.import.extraction_quality_status, "passed");
        assert!(result.import.extraction_quality_score.unwrap_or_default() >= 70);
        let original_path = result
            .import
            .original_path
            .as_deref()
            .expect("PDF original path is recorded");
        assert!(original_path.ends_with(".pdf"));
        assert_eq!(vault::read_bytes(original_path).unwrap(), pdf);
        let source = importer::read_source(&db, &result.import.id).unwrap();
        assert!(source.content.contains("OAuth client credentials"));
        assert!(source.content.contains("short-lived JWT tokens"));
        assert!(source
            .import
            .warnings
            .iter()
            .any(|warning| warning.contains("original PDF was preserved byte-for-byte")));
        drop(roots);
    }

    #[test]
    fn corrupted_pdf_text_is_preserved_but_blocked_from_memory_proposals() {
        let roots = EnvRoots::new("pdf-quality-gate");
        let db = temp_db("pdf-quality-gate");
        let pdf = minimal_text_pdf(
            "E x a m p l e 1 d a y w i n d o w J a n u a r y 1 2 0 2 6 U T C t h r o u g h J a n u a r y 2 2 0 2 6 U T C",
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(&pdf);
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime
            .block_on(importer::import_document(
                &db,
                &DocumentImportRequest {
                    domain: "work".to_string(),
                    input_kind: "file".to_string(),
                    title: "Corrupted spacing".to_string(),
                    content: Some(encoded),
                    content_encoding: Some("base64".to_string()),
                    mime_type: Some("application/pdf".to_string()),
                    source_url: None,
                    file_name: Some("corrupted-spacing.pdf".to_string()),
                },
            ))
            .unwrap();

        assert!(result.proposals.is_empty());
        assert_eq!(result.import.status, "no_candidates");
        assert_eq!(result.import.extraction_quality_status, "failed");
        assert!(result
            .import
            .extraction_quality_issues
            .iter()
            .any(|issue| issue.contains("isolated glyphs")));
        let source = importer::read_source(&db, &result.import.id).unwrap();
        assert!(source.content.contains("PDF extraction blocked"));
        assert!(!source.content.contains("E x a m p l e"));
        assert_eq!(
            vault::read_bytes(result.import.original_path.as_deref().unwrap()).unwrap(),
            pdf
        );
        drop(roots);
    }

    #[test]
    fn document_import_rejects_real_credentials_without_writing_a_source() {
        let roots = EnvRoots::new("document-secret");
        let db = temp_db("document-secret");
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = runtime.block_on(importer::import_document(
            &db,
            &DocumentImportRequest {
                domain: "work".to_string(),
                input_kind: "text".to_string(),
                title: "Unsafe source".to_string(),
                content: Some(
                    "Use Authorization: Bearer real-production-token-1234567890 for every request."
                        .to_string(),
                ),
                content_encoding: None,
                mime_type: None,
                source_url: None,
                file_name: None,
            },
        ));

        assert!(result.is_err());
        assert!(importer::list(&db, None).unwrap().is_empty());
        assert!(!roots.vault.join("_sources").exists());
        drop(roots);
    }

    #[test]
    fn startup_recovery_rolls_forward_an_exact_journaled_proposal() {
        let roots = EnvRoots::new("operation-recovery");
        let db = temp_db("operation-recovery");
        vault::ensure_vault().unwrap();
        let mut request = ManualSaveRequest::basic(
            "work",
            "fact",
            "Recovery invariant",
            "The journaled file must be reconciled into Git, SQLite, and audit after a crash.",
        );
        request.sensitivity = Some("sensitive".to_string());
        let proposal = pipeline::process_manual_save(&db, &request, "manual").unwrap();
        assert_eq!(proposal.status, "pending");
        let operation_id = operations::begin(
            &db,
            "proposal_apply",
            &proposal.id,
            &serde_json::json!({ "finalStatus": "approved" }),
        )
        .unwrap();

        // Simulate a process kill immediately after the atomic file replace.
        vault::write_file_atomic(&proposal.vault_path, &proposal.new_content).unwrap();
        operations::advance(&db, &operation_id, "files_written").unwrap();

        let report = operations::recover(&db).unwrap();
        assert_eq!(report.recovered, 1);
        assert_eq!(report.needs_attention, 0);
        assert_eq!(
            proposals::get_by_id(&db, &proposal.id)
                .unwrap()
                .unwrap()
                .status,
            "approved"
        );
        assert!(index::get_by_id(
            &db,
            &frontmatter::parse(&proposal.new_content).unwrap().0.id
        )
        .unwrap()
        .is_some());
        assert_eq!(
            operations::list(&db)
                .unwrap()
                .into_iter()
                .find(|operation| operation.id == operation_id)
                .unwrap()
                .status,
            "completed"
        );
        assert!(crate::audit::verify_chain(&db).unwrap().ok);
        drop(roots);
    }

    #[test]
    fn startup_recovery_verifies_and_preserves_imported_images() {
        let roots = EnvRoots::new("image-operation-recovery");
        let db = temp_db("image-operation-recovery");
        vault::ensure_vault().unwrap();
        let import_id = uuid::Uuid::new_v4().to_string();
        let source_path = format!("_sources/work/recovered-{import_id}.md");
        let image_path = format!("_sources/work/recovered-{import_id}-diagram.png");
        let snapshot = "# Recovered email\n\n![diagram](recovered-diagram.png)\n";
        let image_bytes = b"test-image-bytes";
        let operation_id = operations::begin(
            &db,
            "document_import",
            &import_id,
            &serde_json::json!({
                "domain": "work",
                "title": "Recovered email",
                "inputKind": "file",
                "sourceRef": "recovered.eml",
                "sourcePath": source_path,
                "originalPath": serde_json::Value::Null,
                "contentHash": crate::audit::compute_content_hash(snapshot),
                "snapshotHash": crate::audit::compute_content_hash(snapshot),
                "byteCount": snapshot.len() as i64,
                "createdAt": "2026-09-23T12:00:00Z",
                "extractionEngine": "mail-parser",
                "extractionVersion": "0.11.5",
                "extractionQualityScore": 100,
                "extractionQualityStatus": "passed",
                "extractionQualityIssues": [],
                "images": [{
                    "path": image_path,
                    "contentHash": format!("{:x}", Sha256::digest(image_bytes)),
                }],
            }),
        )
        .unwrap();

        // Simulate a process kill after every governed file is durable but
        // before Git, SQLite, and audit have been reconciled.
        vault::write_file_atomic(&source_path, snapshot).unwrap();
        vault::write_bytes_atomic(&image_path, image_bytes).unwrap();
        operations::advance(&db, &operation_id, "files_written").unwrap();

        let report = operations::recover(&db).unwrap();
        assert_eq!(report.recovered, 1);
        assert_eq!(report.needs_attention, 0);
        assert_eq!(vault::read_bytes(&image_path).unwrap(), image_bytes);
        assert!(importer::list(&db, Some("work"))
            .unwrap()
            .iter()
            .any(|record| record.id == import_id));
        assert_eq!(
            operations::list(&db)
                .unwrap()
                .into_iter()
                .find(|operation| operation.id == operation_id)
                .unwrap()
                .status,
            "completed"
        );
        assert!(crate::audit::verify_chain(&db).unwrap().ok);
        drop(roots);
    }

    #[test]
    fn expiring_episode_creates_approval_proposals_and_defers_archive() {
        let roots = EnvRoots::new("episode-consolidation");
        let db = temp_db("episode-consolidation");
        vault::ensure_vault().unwrap();
        let mut request = ManualSaveRequest::basic(
            "work",
            "episode",
            "Newsletter production review",
            "## Decision\n\nThe newsletter release must complete quality assurance before publication because production errors affect customers. The release owner is the Editorial Operations team.",
        );
        request.expires = Some(
            (chrono::Utc::now() - chrono::Duration::days(1))
                .format("%Y-%m-%d")
                .to_string(),
        );
        let episode = pipeline::process_manual_save(&db, &request, "manual").unwrap();
        assert_eq!(episode.status, "auto_applied");

        let result = maintenance::run_sweep(&db).unwrap();
        assert!(result.consolidation_proposals >= 1);
        assert_eq!(result.expired, 0);
        assert_eq!(result.deferred_expirations, 1);
        let pending = proposals::list(&db, Some("pending")).unwrap();
        assert!(pending.iter().any(|proposal| {
            proposal.requires_approval
                && proposal.provenance.contains("consolidation:")
                && proposal.new_content.contains("sources:")
                && proposal.new_content.contains("related:")
                && proposal.new_content.contains(&episode.vault_path)
        }));
        let episode_id = frontmatter::parse(&episode.new_content).unwrap().0.id;
        assert_eq!(
            index::get_by_id(&db, &episode_id).unwrap().unwrap().status,
            "active"
        );
        drop(roots);
    }
}
