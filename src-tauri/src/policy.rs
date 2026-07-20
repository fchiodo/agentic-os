use crate::control_models::RiskLevel;

/// Deterministic pre-flight risk gate (ARCHITECTURE.md v1.1 decision #8).
///
/// This is v1 of the policy engine: a keyword heuristic over the task goal,
/// evaluated once before a harness process is ever spawned. It decides
/// (a) whether the task must wait for human approval before running at all,
/// and (b) which Codex sandbox mode to use if/when it runs.
///
/// What this is NOT yet: mid-run interception of individual tool calls
/// inside a running Codex session. That requires wiring into Codex's own
/// approval protocol and is out of scope until the exact JSON-RPC shape is
/// verified against a live authenticated session (see docs/ARCHITECTURE.md
/// harness adapter notes). Every risky verb below still blocks the task
/// pre-flight, so nothing high-risk runs unattended today; it is just a
/// coarser gate than full mid-stream interception would be.
const HIGH_RISK_KEYWORDS: [&str; 14] = [
    "push", "merge", "deploy", "delete", "remove", "drop", "rm -rf", "send email", "send",
    "publish", "pay", "payment", "invoice", "force push",
];

const MEDIUM_RISK_KEYWORDS: [&str; 9] = [
    "commit", "branch", "pull request", "pr", "write", "create file", "modify", "update",
    "install",
];

pub struct PolicyDecision {
    pub risk_level: RiskLevel,
    pub requires_approval: bool,
    pub sandbox_mode: &'static str,
    pub action_summary: String,
}

pub fn evaluate_goal(goal: &str) -> PolicyDecision {
    let lower = goal.to_lowercase();

    let risk_level = if HIGH_RISK_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        RiskLevel::High
    } else if MEDIUM_RISK_KEYWORDS.iter().any(|kw| lower.contains(kw)) {
        RiskLevel::Medium
    } else {
        RiskLevel::Low
    };

    let (requires_approval, sandbox_mode) = match risk_level {
        RiskLevel::Low => (false, "read-only"),
        RiskLevel::Medium => (true, "workspace-write"),
        RiskLevel::High | RiskLevel::Critical => (true, "workspace-write"),
    };

    let action_summary = match risk_level {
        RiskLevel::Low => "Read-only task, auto-approved".to_string(),
        RiskLevel::Medium => "Task may write files in the working directory".to_string(),
        RiskLevel::High => {
            "Task goal mentions a high-risk action (push, delete, send, deploy, or similar)"
                .to_string()
        }
        RiskLevel::Critical => "Task is blocked by policy".to_string(),
    };

    PolicyDecision {
        risk_level,
        requires_approval,
        sandbox_mode,
        action_summary,
    }
}

pub fn derive_title(goal: &str) -> String {
    let trimmed = goal.trim();
    if trimmed.chars().count() <= 64 {
        trimmed.to_string()
    } else {
        let truncated: String = trimmed.chars().take(61).collect();
        format!("{truncated}...")
    }
}
