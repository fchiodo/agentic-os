use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Work,
    Planphysique,
    Personal,
    Family,
    Finance,
    Research,
}

impl Domain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Domain::Work => "work",
            Domain::Planphysique => "planphysique",
            Domain::Personal => "personal",
            Domain::Family => "family",
            Domain::Finance => "finance",
            Domain::Research => "research",
        }
    }

    pub fn parse(value: &str) -> Domain {
        match value {
            "planphysique" => Domain::Planphysique,
            "personal" => Domain::Personal,
            "family" => Domain::Family,
            "finance" => Domain::Finance,
            "research" => Domain::Research,
            _ => Domain::Work,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Harness {
    Codex,
    Claude,
    Acp,
}

impl Harness {
    pub fn as_str(&self) -> &'static str {
        match self {
            Harness::Codex => "codex",
            Harness::Claude => "claude",
            Harness::Acp => "acp",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

impl RiskLevel {
    pub fn as_str(&self) -> &'static str {
        match self {
            RiskLevel::Low => "low",
            RiskLevel::Medium => "medium",
            RiskLevel::High => "high",
            RiskLevel::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum OriginKind {
    Manual,
    Workflow,
    Schedule,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Created,
    Classified,
    Planned,
    Running,
    WaitingForTool,
    WaitingForApproval,
    Resuming,
    Verifying,
    Completed,
    Failed,
    Cancelled,
    PartiallyCompleted,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            TaskStatus::Created => "created",
            TaskStatus::Classified => "classified",
            TaskStatus::Planned => "planned",
            TaskStatus::Running => "running",
            TaskStatus::WaitingForTool => "waiting_for_tool",
            TaskStatus::WaitingForApproval => "waiting_for_approval",
            TaskStatus::Resuming => "resuming",
            TaskStatus::Verifying => "verifying",
            TaskStatus::Completed => "completed",
            TaskStatus::Failed => "failed",
            TaskStatus::Cancelled => "cancelled",
            TaskStatus::PartiallyCompleted => "partially_completed",
        }
    }

    pub fn parse(value: &str) -> TaskStatus {
        match value {
            "classified" => TaskStatus::Classified,
            "planned" => TaskStatus::Planned,
            "running" => TaskStatus::Running,
            "waiting_for_tool" => TaskStatus::WaitingForTool,
            "waiting_for_approval" => TaskStatus::WaitingForApproval,
            "resuming" => TaskStatus::Resuming,
            "verifying" => TaskStatus::Verifying,
            "completed" => TaskStatus::Completed,
            "failed" => TaskStatus::Failed,
            "cancelled" => TaskStatus::Cancelled,
            "partially_completed" => TaskStatus::PartiallyCompleted,
            _ => TaskStatus::Created,
        }
    }

    /// Reserved for the scheduler (Phase 3): terminal tasks are safe to
    /// drop from the in-memory live-event store.
    #[allow(dead_code)]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            TaskStatus::Completed
                | TaskStatus::Failed
                | TaskStatus::Cancelled
                | TaskStatus::PartiallyCompleted
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSummary {
    pub id: String,
    pub title: String,
    pub goal: String,
    pub domain: Domain,
    pub agent_id: Option<String>,
    pub harness: Harness,
    pub status: TaskStatus,
    pub origin_kind: OriginKind,
    pub ontology_category_id: Option<String>,
    pub current_step: i64,
    pub step_count: i64,
    pub cost_tokens: i64,
    pub cost_usd: Option<f64>,
    pub pending_approval_id: Option<String>,
    pub risk_level: RiskLevel,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StepStatus {
    Pending,
    Active,
    Done,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskStep {
    pub index: i64,
    pub title: String,
    pub status: StepStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRef {
    pub id: String,
    pub label: String,
    pub path: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskDetail {
    #[serde(flatten)]
    pub summary: TaskSummary,
    pub plan_version: i64,
    pub steps: Vec<TaskStep>,
    pub artifacts: Vec<ArtifactRef>,
    pub last_event_seq: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskEvent {
    pub task_id: String,
    pub seq: i64,
    pub ts: String,
    pub kind: String,
    pub payload: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewBlock {
    pub kind: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalRequest {
    pub id: String,
    pub task_id: String,
    pub task_title: String,
    pub domain: Domain,
    pub tool_name: String,
    pub action_summary: String,
    pub risk_level: RiskLevel,
    pub preview: Option<PreviewBlock>,
    pub requested_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApprovalDecision {
    pub id: String,
    pub decision: String, // "approve" | "deny"
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSubmitRequest {
    pub goal: String,
    pub domain: Option<String>,
    pub agent_id: Option<String>,
    pub cwd: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlStatus {
    pub pending_approvals: i64,
    pub pending_memory_proposals: i64,
    pub running_tasks: i64,
    pub spent_today_usd: f64,
    pub audit_chain_ok: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AuditRunSummary {
    pub run_id: String,
    pub task_id: Option<String>,
    pub title: String,
    pub ts: String,
    pub status: String,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TraceEntry {
    pub run_id: String,
    pub seq: i64,
    pub ts: String,
    pub kind: String,
    pub summary: String,
    pub detail: Value,
    pub tokens: Option<i64>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AuditChainStatus {
    pub ok: bool,
    #[serde(rename = "checkedRows")]
    pub checked_rows: i64,
    #[serde(rename = "brokenAt", skip_serializing_if = "Option::is_none")]
    pub broken_at: Option<String>,
}
