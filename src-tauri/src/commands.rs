use tauri::{AppHandle, State};

use crate::control_models::ControlStatus;
use crate::db::Db;
use crate::models::DashboardSnapshot;
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
    let pending_memory_proposals =
        memory::proposals::list(&db, Some("pending"))
            .map(|p| p.len() as i64)
            .unwrap_or(0);

    Ok(ControlStatus {
        pending_memory_proposals,
    })
}

// ---------------------------------------------------------------------------
// Memory commands (Phase 2 — Second Brain)
// ---------------------------------------------------------------------------

use crate::memory;
use crate::memory::{
    DocumentImportRecord, DocumentImportRequest, DocumentImportResult, DocumentSourceReadResult,
    MaintenanceResult, ManualSaveRequest, MemoryAnswer, MemoryAnswerFeedbackRequest,
    MemoryAskRequest, MemoryIngestRequest, MemoryIngestResult, MemoryOperationRecord,
    MemoryReadResult, MemorySearchOpts, MemoryWriteProposal, ProposalDecideRequest, ReindexResult,
    RetrievalBenchmarkReport, RetrievalEvalCase, RetrievalEvalCaseRequest, ScoredMemory, VaultNode,
};

#[tauri::command]
pub fn memory_tree(db: State<'_, Db>, domain: Option<String>) -> Result<Vec<VaultNode>, String> {
    memory::vault::ensure_vault().map_err(|e| e.to_string())?;
    memory::index::ensure_tables(&db).map_err(|e| e.to_string())?;
    let mut nodes = memory::vault::tree(domain.as_deref()).map_err(|e| e.to_string())?;

    fn enrich(db: &Db, nodes: &mut [VaultNode]) -> Result<(), String> {
        for node in nodes {
            if node.is_dir {
                enrich(db, &mut node.children)?;
            } else if let Some((id, mem_type, status)) =
                memory::index::metadata_by_path(db, &node.path).map_err(|e| e.to_string())?
            {
                node.memory_id = Some(id);
                node.mem_type = Some(mem_type);
                node.status = Some(status);
            }
        }
        Ok(())
    }

    enrich(&db, &mut nodes)?;
    Ok(nodes)
}

#[tauri::command]
pub fn memory_read(db: State<'_, Db>, path: String) -> Result<MemoryReadResult, String> {
    memory::vault::ensure_vault().map_err(|e| e.to_string())?;
    if let Some(source) =
        memory::importer::read_source_by_path(&db, &path).map_err(|e| e.to_string())?
    {
        return Ok(MemoryReadResult {
            frontmatter: None,
            markdown: source.content,
            status: "active".to_string(),
            git_last_commit: source.git_last_commit,
        });
    }
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

/// In-flight Ask runs keyed by the frontend-generated ask id, so a Stop
/// click can cancel exactly the run it belongs to. Watch channels let one
/// Stop reach whichever model turn (planning or synthesis) is active.
#[derive(Default)]
pub struct AskCancellations(
    pub std::sync::Mutex<std::collections::HashMap<String, tokio::sync::watch::Sender<bool>>>,
);

#[tauri::command]
pub async fn memory_ask(
    db: State<'_, Db>,
    cancellations: State<'_, AskCancellations>,
    ask_id: String,
    request: MemoryAskRequest,
    on_progress: tauri::ipc::Channel<memory::MemoryAskProgress>,
) -> Result<MemoryAnswer, String> {
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    cancellations
        .0
        .lock()
        .expect("ask cancellation registry poisoned")
        .insert(ask_id.clone(), cancel_tx);

    let db = db.inner().clone();
    let result = memory::retrieval::ask(
        &db,
        &request,
        move |event| {
            // A closed or slow listener must never fail the ask itself.
            let _ = on_progress.send(event);
        },
        cancel_rx,
    )
    .await;

    cancellations
        .0
        .lock()
        .expect("ask cancellation registry poisoned")
        .remove(&ask_id);
    result.map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_ask_cancel(
    cancellations: State<'_, AskCancellations>,
    ask_id: String,
) -> Result<(), String> {
    if let Some(cancel_tx) = cancellations
        .0
        .lock()
        .expect("ask cancellation registry poisoned")
        .remove(&ask_id)
    {
        let _ = cancel_tx.send(true);
    }
    Ok(())
}

#[tauri::command]
pub async fn memory_lint(
    db: State<'_, Db>,
    domain: Option<String>,
    deep: Option<bool>,
) -> Result<memory::MemoryLintReport, String> {
    let db = db.inner().clone();
    memory::lint::run_lint(&db, domain.as_deref(), deep.unwrap_or(false))
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_answer_feedback(
    db: State<'_, Db>,
    request: MemoryAnswerFeedbackRequest,
) -> Result<(), String> {
    memory::retrieval::record_answer_feedback(&db, &request).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_save_manual(
    db: State<'_, Db>,
    request: ManualSaveRequest,
) -> Result<MemoryWriteProposal, String> {
    memory::pipeline::process_manual_save(&db, &request, "manual").map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_ingest(
    db: State<'_, Db>,
    request: MemoryIngestRequest,
) -> Result<MemoryIngestResult, String> {
    memory::pipeline::process_ingest_batch(&db, &request).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn memory_import_document(
    app: AppHandle,
    db: State<'_, Db>,
    request: DocumentImportRequest,
) -> Result<DocumentImportResult, String> {
    let db = db.inner().clone();
    memory::importer::import_document_with_app(&db, &request, &app)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_document_imports_list(
    db: State<'_, Db>,
    domain: Option<String>,
) -> Result<Vec<DocumentImportRecord>, String> {
    memory::importer::list(&db, domain.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_document_source_read(
    db: State<'_, Db>,
    id: String,
) -> Result<DocumentSourceReadResult, String> {
    memory::importer::read_source(&db, &id).map_err(|e| e.to_string())
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

#[tauri::command]
pub fn memory_operations_list(db: State<'_, Db>) -> Result<Vec<MemoryOperationRecord>, String> {
    memory::operations::list(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_retrieval_benchmark(db: State<'_, Db>) -> Result<RetrievalBenchmarkReport, String> {
    memory::retrieval::benchmark(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_retrieval_eval_cases_list(
    db: State<'_, Db>,
) -> Result<Vec<RetrievalEvalCase>, String> {
    memory::retrieval::list_eval_cases(&db).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_retrieval_eval_case_save(
    db: State<'_, Db>,
    request: RetrievalEvalCaseRequest,
) -> Result<RetrievalEvalCase, String> {
    memory::retrieval::save_eval_case(&db, &request).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn memory_orbit_map(
    db: State<'_, Db>,
    domain: Option<String>,
    include_sensitive: Option<bool>,
    activity_window: Option<String>,
) -> Result<crate::orbit::OrbitMap, String> {
    crate::orbit::build(
        db.inner(),
        domain.as_deref(),
        include_sensitive.unwrap_or(false),
        activity_window.as_deref(),
    )
    .map_err(|error| error.to_string())
}
