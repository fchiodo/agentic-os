use tauri::{AppHandle, State};

use crate::approval;
use crate::audit;
use crate::control_models::{
    ApprovalDecision, ApprovalRequest, AuditChainStatus, AuditRunSummary, ControlStatus, Domain,
    Harness, OriginKind, PreviewBlock, TaskDetail, TaskEvent, TaskStatus, TaskSubmitRequest,
    TaskSummary, TraceEntry,
};
use crate::db::Db;
use crate::harness::codex as codex_harness;
use crate::models::DashboardSnapshot;
use crate::orchestrator;
use crate::policy;
use crate::snapshot;

#[tauri::command]
pub fn get_app_snapshot() -> Result<DashboardSnapshot, String> {
    snapshot::load_snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn refresh_app_snapshot() -> Result<DashboardSnapshot, String> {
    snapshot::load_snapshot().map_err(|error| error.to_string())
}

#[tauri::command]
pub fn control_status(db: State<'_, Db>) -> Result<ControlStatus, String> {
    let pending_approvals =
        approval::list_pending(&db).map_err(|e| e.to_string())?.len() as i64;
    let running_tasks = orchestrator::running_task_count(&db).map_err(|e| e.to_string())?;
    let spent_today_usd = orchestrator::spent_today_usd(&db).map_err(|e| e.to_string())?;
    let audit_chain_ok = audit::verify_chain(&db).map_err(|e| e.to_string())?.ok;

    let pending_memory_proposals =
        memory::proposals::list(&db, Some("pending"))
            .map(|p| p.len() as i64)
            .unwrap_or(0);

    Ok(ControlStatus {
        pending_approvals,
        pending_memory_proposals,
        running_tasks,
        spent_today_usd,
        audit_chain_ok,
    })
}

#[tauri::command]
pub fn tasks_list(db: State<'_, Db>) -> Result<Vec<TaskSummary>, String> {
    orchestrator::list_tasks(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn tasks_get(db: State<'_, Db>, id: String) -> Result<TaskDetail, String> {
    orchestrator::get_detail(&db, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("task {id} not found"))
}

#[tauri::command]
pub fn tasks_events_since(
    db: State<'_, Db>,
    id: String,
    since_seq: i64,
) -> Result<Vec<TaskEvent>, String> {
    orchestrator::events_since(&db, &id, since_seq).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn tasks_submit(
    app: AppHandle,
    db: State<'_, Db>,
    request: TaskSubmitRequest,
) -> Result<TaskSummary, String> {
    let db = db.inner().clone();
    let domain = request
        .domain
        .as_deref()
        .map(Domain::parse)
        .unwrap_or(Domain::Work);

    // No workspace picker yet (out of scope for Phase 1, see UI-SPEC.md);
    // default to the user's home directory, which is always a safe,
    // read-only-first working root for codex exec.
    let cwd = request.cwd.clone().unwrap_or_else(|| {
        dirs::home_dir()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string())
    });

    let decision = policy::evaluate_goal(&request.goal);

    let summary = orchestrator::create_task(
        &db,
        &request.goal,
        domain,
        Harness::Codex,
        request.agent_id.clone(),
        &cwd,
        OriginKind::Manual,
        &decision,
    )
    .map_err(|e| e.to_string())?;

    audit::append_row(
        &db,
        &summary.id,
        &summary.id,
        "input",
        "Task submitted",
        &serde_json::json!({ "goal": request.goal, "domain": domain.as_str() }),
        None,
        None,
    )
    .map_err(|e| e.to_string())?;
    audit::append_row(
        &db,
        &summary.id,
        &summary.id,
        "policy_decision",
        &decision.action_summary,
        &serde_json::json!({
            "riskLevel": decision.risk_level.as_str(),
            "sandboxMode": decision.sandbox_mode,
            "requiresApproval": decision.requires_approval,
        }),
        None,
        None,
    )
    .map_err(|e| e.to_string())?;

    if decision.requires_approval {
        let preview = Some(PreviewBlock {
            kind: "text".to_string(),
            content: request.goal.clone(),
        });
        approval::create_approval(
            &db,
            &summary.id,
            domain,
            "codex.exec",
            &decision.action_summary,
            decision.risk_level,
            preview,
        )
        .map_err(|e| e.to_string())?;
    } else {
        let db_spawn = db.clone();
        let app_spawn = app.clone();
        let task_id = summary.id.clone();
        tauri::async_runtime::spawn(async move {
            codex_harness::spawn_and_stream(app_spawn, db_spawn, task_id).await;
        });
    }

    orchestrator::get_summary(&db, &summary.id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "task vanished after creation".to_string())
}

#[tauri::command]
pub fn tasks_cancel(db: State<'_, Db>, id: String) -> Result<TaskSummary, String> {
    orchestrator::set_status(&db, &id, TaskStatus::Cancelled).map_err(|e| e.to_string())?;
    orchestrator::set_failure_reason(&db, &id, "Cancelled by user").map_err(|e| e.to_string())?;
    orchestrator::get_summary(&db, &id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("task {id} not found"))
}

#[tauri::command]
pub fn approvals_list(db: State<'_, Db>) -> Result<Vec<ApprovalRequest>, String> {
    approval::list_pending(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn approvals_decide(
    app: AppHandle,
    db: State<'_, Db>,
    decision: ApprovalDecision,
) -> Result<ApprovalRequest, String> {
    let db = db.inner().clone();
    let approval_before =
        approval::get_approval(&db, &decision.id)
            .map_err(|e| e.to_string())?
            .ok_or_else(|| format!("approval {} not found", decision.id))?;

    let task_id =
        approval::decide(&db, &decision.id, &decision.decision, decision.note.as_deref())
            .map_err(|e| e.to_string())?;

    orchestrator::set_pending_approval(&db, &task_id, None).map_err(|e| e.to_string())?;

    audit::append_row(
        &db,
        &task_id,
        &task_id,
        "approval",
        &format!("Approval {}", decision.decision),
        &serde_json::json!({
            "approvalId": decision.id,
            "decision": decision.decision,
            "note": decision.note,
        }),
        None,
        None,
    )
    .map_err(|e| e.to_string())?;

    if decision.decision == "approve" {
        orchestrator::set_status(&db, &task_id, TaskStatus::Planned).map_err(|e| e.to_string())?;
        let db_spawn = db.clone();
        let app_spawn = app.clone();
        let task_id_spawn = task_id.clone();
        tauri::async_runtime::spawn(async move {
            codex_harness::spawn_and_stream(app_spawn, db_spawn, task_id_spawn).await;
        });
    } else {
        orchestrator::set_status(&db, &task_id, TaskStatus::Cancelled).map_err(|e| e.to_string())?;
        orchestrator::set_failure_reason(
            &db,
            &task_id,
            decision.note.as_deref().unwrap_or("Denied by user"),
        )
        .map_err(|e| e.to_string())?;
    }

    Ok(ApprovalRequest {
        id: decision.id,
        task_id: approval_before.task_id,
        task_title: approval_before.task_title,
        domain: approval_before.domain,
        tool_name: approval_before.tool_name,
        action_summary: approval_before.action_summary,
        risk_level: approval_before.risk_level,
        preview: approval_before.preview,
        requested_at: approval_before.requested_at,
    })
}

#[tauri::command]
pub fn audit_runs(db: State<'_, Db>) -> Result<Vec<AuditRunSummary>, String> {
    let tasks = orchestrator::list_tasks(&db).map_err(|e| e.to_string())?;
    Ok(tasks
        .into_iter()
        .map(|t| AuditRunSummary {
            run_id: t.id.clone(),
            task_id: Some(t.id),
            title: t.title,
            ts: t.updated_at,
            status: t.status.as_str().to_string(),
            cost_usd: t.cost_usd,
        })
        .collect())
}

#[tauri::command]
pub fn audit_trace(db: State<'_, Db>, run_id: String) -> Result<Vec<TraceEntry>, String> {
    audit::read_trace(&db, &run_id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn audit_verify_chain(db: State<'_, Db>) -> Result<AuditChainStatus, String> {
    audit::verify_chain(&db).map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Memory commands (Phase 2 — Second Brain)
// ---------------------------------------------------------------------------

use crate::memory;
use crate::memory::{
    MaintenanceResult, ManualSaveRequest, MemoryReadResult, MemorySearchOpts, MemoryWriteProposal,
    ProposalDecideRequest, ReindexResult, ScoredMemory, VaultNode,
};

#[tauri::command]
pub fn memory_tree(domain: Option<String>) -> Result<Vec<VaultNode>, String> {
    memory::vault::ensure_vault().map_err(|e| e.to_string())?;
    memory::vault::tree(domain.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_read(db: State<'_, Db>, path: String) -> Result<MemoryReadResult, String> {
    memory::vault::ensure_vault().map_err(|e| e.to_string())?;
    let (content, _full_path) =
        memory::vault::read_file(&path).map_err(|e| e.to_string())?;
    let git_last_commit = memory::vault::git_last_commit(&path);

    let (fm, body) = match memory::frontmatter::parse(&content) {
        Some((fm, body)) => (Some(fm), body),
        None => (None, content),
    };

    // Real status from the index (stale/expired flags live there, driven
    // by the maintenance sweep); active for unindexed files.
    let status = fm
        .as_ref()
        .and_then(|f| memory::index::get_by_id(&db, &f.id).ok().flatten())
        .map(|row| row.status)
        .unwrap_or_else(|| "active".to_string());

    Ok(MemoryReadResult {
        frontmatter: fm,
        markdown: body,
        status,
        git_last_commit,
    })
}

#[tauri::command]
pub fn memory_search(
    db: State<'_, Db>,
    query: String,
    domain: Option<String>,
    opts: Option<MemorySearchOpts>,
) -> Result<Vec<ScoredMemory>, String> {
    let search_opts = opts.unwrap_or(MemorySearchOpts {
        include_stale: true,
        limit: Some(8),
    });
    memory::retrieval::search(&db, &query, domain.as_deref(), &search_opts)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_save_manual(
    db: State<'_, Db>,
    request: ManualSaveRequest,
) -> Result<MemoryWriteProposal, String> {
    memory::pipeline::process_manual_save(&db, &request, "manual").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_proposals_list(
    db: State<'_, Db>,
    status: Option<String>,
) -> Result<Vec<MemoryWriteProposal>, String> {
    memory::proposals::list(&db, status.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_proposals_decide(
    db: State<'_, Db>,
    request: ProposalDecideRequest,
) -> Result<MemoryWriteProposal, String> {
    memory::proposals::decide(&db, &request.id, &request.decision).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_confirm(db: State<'_, Db>, id: String) -> Result<(), String> {
    memory::index::confirm(&db, &id).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_reindex(db: State<'_, Db>) -> Result<ReindexResult, String> {
    memory::vault::ensure_vault().map_err(|e| e.to_string())?;
    memory::index::reindex(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_maintenance_run(db: State<'_, Db>) -> Result<MaintenanceResult, String> {
    memory::vault::ensure_vault().map_err(|e| e.to_string())?;
    memory::maintenance::run_sweep(&db).map_err(|e| e.to_string())
}

/// Distill a completed run into a candidate skill (MEMORY-SPEC §4 source 4).
/// Always returns a pending proposal — approval happens in the Memory page.
#[tauri::command]
pub fn skills_distill(db: State<'_, Db>, task_id: String) -> Result<MemoryWriteProposal, String> {
    let detail = orchestrator::get_detail(&db, &task_id)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("task {task_id} not found"))?;

    if detail.summary.status != TaskStatus::Completed {
        return Err("only completed tasks can be distilled into skills".to_string());
    }

    let step_titles: Vec<String> = detail.steps.iter().map(|s| s.title.clone()).collect();

    memory::pipeline::process_skill_distill(
        &db,
        &task_id,
        detail.summary.domain.as_str(),
        &detail.summary.title,
        &detail.summary.goal,
        &step_titles,
    )
    .map_err(|e| e.to_string())
}
