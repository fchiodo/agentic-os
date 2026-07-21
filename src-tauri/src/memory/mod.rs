pub mod context;
pub mod frontmatter;
pub mod importer;
pub mod index;
pub mod maintenance;
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
}

impl MemoryType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryType::Fact => "fact",
            MemoryType::Decision => "decision",
            MemoryType::Preference => "preference",
            MemoryType::Entity => "entity",
            MemoryType::Episode => "episode",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "fact" => Some(MemoryType::Fact),
            "decision" => Some(MemoryType::Decision),
            "preference" => Some(MemoryType::Preference),
            "entity" => Some(MemoryType::Entity),
            "episode" => Some(MemoryType::Episode),
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
    pub confidence: f64,
    pub sensitivity: Sensitivity,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_from: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryAnswer {
    pub answer: String,
    pub citations: Vec<MemoryCitation>,
    pub warnings: Vec<String>,
    pub abstained: bool,
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
    pub content_hash: String,
    pub byte_count: i64,
    pub candidate_count: i64,
    pub warning_count: i64,
    pub warnings: Vec<String>,
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
            confidence: row.confidence,
            sensitivity: Sensitivity::Normal,
            valid_from: None,
            valid_until: None,
            stale_after_days: Some(180),
            last_confirmed: Some(old),
            confirmations: Some(1),
            expires: None,
            tags: vec![],
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
    fn ask_memory_cites_evidence_and_abstains_without_it() {
        let roots = EnvRoots::new("ask");
        let db = temp_db("ask");
        let request = ManualSaveRequest::basic(
            "work",
            "decision",
            "PowerReviews feed mode",
            "The PowerReviews feed is delta because full files exceed the SFTP timeout.",
        );
        pipeline::process_manual_save(&db, &request, "manual").unwrap();

        let answer = retrieval::ask(
            &db,
            &MemoryAskRequest {
                question: "Why is the PowerReviews feed delta?".to_string(),
                domain: "work".to_string(),
                include_stale: false,
            },
        )
        .unwrap();
        assert!(!answer.abstained);
        assert_eq!(answer.citations.len(), 1);
        assert!(answer.answer.contains("[1]"));

        let absent = retrieval::ask(
            &db,
            &MemoryAskRequest {
                question: "What is the lunar office policy?".to_string(),
                domain: "work".to_string(),
                include_stale: false,
            },
        )
        .unwrap();
        assert!(absent.abstained);
        assert!(absent.citations.is_empty());
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
        assert_eq!(value.get("memType").and_then(|value| value.as_str()), Some("fact"));
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
            proposals::get_by_id(&db, &pending.id).unwrap().unwrap().status,
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
                source_url: None,
                file_name: None,
            },
        ));

        assert!(result.is_err());
        assert!(importer::list(&db, None).unwrap().is_empty());
        assert!(!roots.vault.join("_sources").exists());
        drop(roots);
    }
}
