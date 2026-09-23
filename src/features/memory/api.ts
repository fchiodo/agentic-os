import { Channel, invoke } from '@tauri-apps/api/core'
import {
  documentImportRecordSchema,
  documentImportResultSchema,
  documentSourceReadResultSchema,
  maintenanceResultSchema,
  memoryOperationRecordSchema,
  memoryAnswerSchema,
  memoryLintReportSchema,
  memoryAskProgressSchema,
  memoryIngestResultSchema,
  memoryReadResultSchema,
  memoryWriteProposalSchema,
  orbitMapSchema,
  reindexResultSchema,
  retrievalBenchmarkReportSchema,
  retrievalEvalCaseSchema,
  scoredMemorySchema,
  vaultNodeSchema,
  type DocumentImportRecord,
  type DocumentImportRequest,
  type DocumentImportResult,
  type DocumentSourceReadResult,
  type ManualSaveRequest,
  type MaintenanceResult,
  type MemoryAnswer,
  type MemoryAnswerFeedbackRequest,
  type MemoryAskProgress,
  type MemoryOperationRecord,
  type MemoryAskRequest,
  type MemoryIngestRequest,
  type MemoryIngestResult,
  type MemoryLintReport,
  type MemoryReadResult,
  type MemorySearchOpts,
  type MemoryWriteProposal,
  type OrbitMap,
  type OrbitActivityWindow,
  type ProposalDecideRequest,
  type ReindexResult,
  type RetrievalBenchmarkReport,
  type RetrievalEvalCase,
  type RetrievalEvalCaseRequest,
  type ScoredMemory,
  type VaultNode,
} from '@/features/memory/schema'
import { isTauriRuntime } from '@/lib/tauri'

// ---------------------------------------------------------------------------
// Mock data for non-Tauri dev
// ---------------------------------------------------------------------------

const mockVaultTree: VaultNode[] = [
  {
    name: 'work',
    path: 'work',
    isDir: true,
    memoryId: null,
    memType: null,
    status: null,
    children: [
      {
        name: 'decisions',
        path: 'work/decisions',
        isDir: true,
        memoryId: null,
        memType: null,
        status: null,
        children: [
          {
            name: '2026-07-20-powerreviews-feed-delta.md',
            path: 'work/decisions/2026-07-20-powerreviews-feed-delta.md',
            isDir: false,
            memoryId: 'mem-001',
            memType: 'decision',
            status: 'active',
            children: [],
          },
        ],
      },
      {
        name: 'projects',
        path: 'work/projects',
        isDir: true,
        memoryId: null,
        memType: null,
        status: null,
        children: [],
      },
      {
        name: 'meetings',
        path: 'work/meetings',
        isDir: true,
        memoryId: null,
        memType: null,
        status: null,
        children: [
          {
            name: '2026-07-18-databricks-sync.md',
            path: 'work/meetings/2026-07-18-databricks-sync.md',
            isDir: false,
            memoryId: 'mem-002',
            memType: 'episode',
            status: 'active',
            children: [],
          },
        ],
      },
    ],
  },
  {
    name: 'personal',
    path: 'personal',
    isDir: true,
    memoryId: null,
    memType: null,
    status: null,
    children: [],
  },
]

const mockReadResult: MemoryReadResult = {
  frontmatter: {
    id: 'mem-001',
    memType: 'decision',
    domain: 'work',
    title: 'PowerReviews feed is delta, not full',
    created: '2026-07-20T09:12:00Z',
    updated: '2026-07-20T09:12:00Z',
    provenance: { source: 'task:4b1e', ts: '2026-07-20T09:12:00Z' },
    sources: [],
    confidence: 0.9,
    sensitivity: 'normal',
    validFrom: null,
    validUntil: null,
    staleAfterDays: null,
    lastConfirmed: '2026-07-20T09:12:00Z',
    confirmations: 1,
    expires: null,
    tags: ['powerreviews', 'voc', 'sftp'],
    related: ['work/meetings/2026-07-18-databricks-sync.md'],
  },
  markdown:
    'Delta feed daily instead of full: full files >2GB hit the SFTP timeout.\nDecided with the vendor on the 2026-06-12 call. Open point: retention of processed files.',
  status: 'active',
  gitLastCommit: 'a1b2c3d',
}

const mockSearchResults: ScoredMemory[] = [
  {
    row: {
      id: 'mem-001',
      vaultPath: 'work/decisions/2026-07-20-powerreviews-feed-delta.md',
      domain: 'work',
      memType: 'decision',
      title: 'PowerReviews feed is delta, not full',
      summary: 'Delta feed daily instead of full',
      sensitivity: 'normal',
      confidence: 0.9,
      createdAt: '2026-07-20T09:12:00Z',
      updatedAt: '2026-07-20T09:12:00Z',
      validFrom: null,
      validUntil: null,
      staleAfterDays: null,
      lastConfirmedAt: '2026-07-20T09:12:00Z',
      confirmationCount: 1,
      lastAccessedAt: null,
      accessCount: 0,
      expiresAt: null,
      provenance: '{"source":"task:4b1e","ts":"2026-07-20T09:12:00Z"}',
      contentHash: 'abc123',
      status: 'active',
    },
    score: 0.87,
    relevance: 0.92,
    recency: 0.95,
    trust: 0.84,
  },
]

function cloneMockValue<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function buildMockProposals(): MemoryWriteProposal[] {
  const now = new Date().toISOString()
  return [
    {
      id: 'proposal-mock-pending-powerreviews',
      taskId: null,
      vaultPath: 'work/memories/powerreviews-decision.md',
      domain: 'work',
      kind: 'memory',
      op: 'supersede',
      supersedesId: 'mem-001',
      sensitivity: 'sensitive',
      unifiedDiff: [
        '--- a/work/memories/powerreviews-decision.md',
        '+++ b/work/memories/powerreviews-decision.md',
        '@@',
        '- real-time sync',
        '+ nightly sync, flagged for March',
      ].join('\n'),
      newContent: [
        'id: mem-001',
        'title: Update PowerReviews feed decision',
        'domain: work',
        'memType: decision',
        '',
        'Nightly sync, flagged for March.',
      ].join('\n'),
      provenance: '{"source":"manual","ts":"2026-07-21T10:00:00Z"}',
      gateReport: JSON.stringify({
        passed: true,
        checks: [
          {
            name: 'truth change',
            passed: true,
            detail: 'This write changes an existing decision and must be reviewed.',
          },
          {
            name: 'domain isolation',
            passed: true,
            detail: 'The proposed change stays inside the work domain.',
          },
        ],
      }),
      requiresApproval: true,
      status: 'pending',
      createdAt: now,
      decidedAt: null,
      baseContentHash: 'mock-base-hash-001',
      importId: null,
    },
    {
      id: 'proposal-mock-approved-audit-window',
      taskId: null,
      vaultPath: 'work/facts/audit-window.md',
      domain: 'work',
      kind: 'memory',
      op: 'create',
      supersedesId: null,
      sensitivity: 'normal',
      unifiedDiff: [
        '--- /dev/null',
        '+++ b/work/facts/audit-window.md',
        '@@',
        '+ retain 30 days of nominal audit events',
      ].join('\n'),
      newContent: [
        'id: mem-003',
        'title: Audit retention window',
        'domain: work',
        'memType: fact',
        '',
        'Retain 30 days of nominal audit events.',
      ].join('\n'),
      provenance: '{"source":"manual","ts":"2026-07-20T15:20:00Z"}',
      gateReport: JSON.stringify({
        passed: true,
        checks: [
          {
            name: 'provenance',
            passed: true,
            detail: 'The source metadata is present and verifiable.',
          },
        ],
      }),
      requiresApproval: true,
      status: 'approved',
      createdAt: now,
      decidedAt: now,
      baseContentHash: null,
      importId: null,
    },
  ]
}

let mockProposals = buildMockProposals()
let mockDocumentImports: DocumentImportRecord[] = []
const mockCancelledAsks = new Set<string>()

export function resetMockMemoryState() {
  mockProposals = buildMockProposals()
  mockDocumentImports = []
  mockCancelledAsks.clear()
}

// ---------------------------------------------------------------------------
// API functions
// ---------------------------------------------------------------------------

export async function memoryTree(domain?: string): Promise<VaultNode[]> {
  if (!isTauriRuntime()) {
    return domain
      ? mockVaultTree.filter((n) => n.name === domain)
      : mockVaultTree
  }
  const payload = await invoke<VaultNode[]>('memory_tree', { domain: domain ?? null })
  return vaultNodeSchema.array().parse(payload)
}

export async function memoryRead(path: string): Promise<MemoryReadResult> {
  if (!isTauriRuntime()) {
    return mockReadResult
  }
  const payload = await invoke<MemoryReadResult>('memory_read', { path })
  return memoryReadResultSchema.parse(payload)
}

export async function memorySearch(
  query: string,
  domain?: string,
  opts?: MemorySearchOpts,
): Promise<ScoredMemory[]> {
  if (!isTauriRuntime()) {
    return mockSearchResults
  }
  const payload = await invoke<ScoredMemory[]>('memory_search', {
    query,
    domain: domain ?? null,
    opts: opts ?? { includeStale: true, limit: 8 },
  })
  return scoredMemorySchema.array().parse(payload)
}

/**
 * Matches the backend's STOPPED_BY_USER constant in harness/structured.rs.
 * A rejection carrying this message is a user action, not a failure.
 */
export const ASK_STOPPED_MESSAGE = 'Ask stopped by user'

export function isAskStoppedError(error: unknown): boolean {
  return error instanceof Error
    ? error.message.includes(ASK_STOPPED_MESSAGE)
    : String(error).includes(ASK_STOPPED_MESSAGE)
}

export async function memoryAsk(
  request: MemoryAskRequest,
  askId: string,
  onProgress?: (event: MemoryAskProgress) => void,
): Promise<MemoryAnswer> {
  if (!isTauriRuntime()) {
    const mockStages: Array<[MemoryAskProgress['stage'], string]> = [
      ['retrieval', 'Searching the vault for relevant passages'],
      ['retrieval', '1 relevant passage found'],
      ['synthesis', 'Starting the AI synthesis turn'],
      ['synthesis', 'Model is reasoning over the evidence'],
      ['verification', 'Verifying every claim against its citations'],
    ]
    for (const [stage, label] of mockStages) {
      if (mockCancelledAsks.delete(askId)) {
        throw new Error(ASK_STOPPED_MESSAGE)
      }
      onProgress?.({ stage, label, at: new Date().toISOString(), transient: false })
      await new Promise((resolve) => setTimeout(resolve, 80))
    }
    if (mockCancelledAsks.delete(askId)) {
      throw new Error(ASK_STOPPED_MESSAGE)
    }
    return {
      id: '00000000-0000-4000-8000-000000000001',
      question: request.question,
      domain: request.domain,
      answer:
        'Delta feed daily instead of full: full files over **2GB** hit the SFTP timeout. [1] The decision was made with the vendor on the 2026-06-12 call. [1] Retention of processed files is still an open point and was raised again in the Databricks sync. [2]',
      citations: [
        {
          id: 'mem-001',
          number: 1,
          title: 'PowerReviews feed is delta, not full',
          vaultPath: 'work/decisions/2026-07-20-powerreviews-feed-delta.md',
          status: 'active',
          excerpt: 'Delta feed daily instead of full: full files over 2GB hit the SFTP timeout.',
          score: 0.87,
          sourceKind: 'memory',
        },
        {
          id: 'mem-002',
          number: 2,
          title: 'Databricks sync notes',
          vaultPath: 'work/meetings/2026-07-18-databricks-sync.md',
          status: 'active',
          excerpt: 'Open point: retention of processed files.',
          score: 0.74,
          sourceKind: 'memory',
        },
      ],
      warnings: [],
      abstained: false,
      confidence: 'medium',
      confidenceScore: 0.77,
      sourceCount: 1,
      model: 'Codex',
      generatedAt: new Date().toISOString(),
    }
  }
  const channel = new Channel<unknown>()
  channel.onmessage = (message) => {
    const parsed = memoryAskProgressSchema.safeParse(message)
    if (parsed.success) {
      onProgress?.(parsed.data)
    }
  }
  const payload = await invoke<MemoryAnswer>('memory_ask', {
    askId,
    request,
    onProgress: channel,
  })
  return memoryAnswerSchema.parse(payload)
}

export async function memoryAskCancel(askId: string): Promise<void> {
  if (!isTauriRuntime()) {
    mockCancelledAsks.add(askId)
    return
  }
  await invoke('memory_ask_cancel', { askId })
}

export async function memoryAnswerFeedback(
  request: MemoryAnswerFeedbackRequest,
): Promise<void> {
  if (!isTauriRuntime()) return
  await invoke('memory_answer_feedback', { request })
}

export async function memoryIngest(
  request: MemoryIngestRequest,
): Promise<MemoryIngestResult> {
  if (!isTauriRuntime()) {
    return { proposals: [], rejected: [] }
  }
  const payload = await invoke<MemoryIngestResult>('memory_ingest', { request })
  return memoryIngestResultSchema.parse(payload)
}

export async function memorySaveManual(
  request: ManualSaveRequest,
): Promise<MemoryWriteProposal> {
  if (!isTauriRuntime()) {
    const proposal: MemoryWriteProposal = {
      id: `proposal-mock-${Date.now()}`,
      taskId: null,
      vaultPath: `memories/${Date.now()}.md`,
      domain: request.domain,
      kind: 'memory',
      op: 'create',
      supersedesId: null,
      sensitivity: 'normal',
      unifiedDiff: `+${request.body.length}`,
      newContent: '',
      provenance: '{"source":"manual","ts":"2026-07-20T00:00:00Z"}',
      gateReport: '{"checks":[],"passed":true}',
      requiresApproval: false,
      status: 'auto_applied',
      createdAt: new Date().toISOString(),
      decidedAt: null,
      baseContentHash: null,
      importId: null,
    }
    mockProposals = [proposal, ...mockProposals]
    return cloneMockValue(proposal)
  }
  const payload = await invoke<MemoryWriteProposal>('memory_save_manual', { request })
  return memoryWriteProposalSchema.parse(payload)
}

export async function memoryProposalsList(
  status?: string,
): Promise<MemoryWriteProposal[]> {
  if (!isTauriRuntime()) {
    return cloneMockValue(
      status
        ? mockProposals.filter((proposal) => proposal.status === status)
        : mockProposals,
    )
  }
  const payload = await invoke<MemoryWriteProposal[]>('memory_proposals_list', {
    status: status ?? null,
  })
  return memoryWriteProposalSchema.array().parse(payload)
}

export async function memoryImportDocument(
  request: DocumentImportRequest,
): Promise<DocumentImportResult> {
  if (!isTauriRuntime()) {
    const now = new Date().toISOString()
    const importId = `import-mock-${Date.now()}`
    const proposal: MemoryWriteProposal = {
      id: `proposal-${importId}`,
      taskId: null,
      vaultPath: `${request.domain}/memories/${Date.now()}.md`,
      domain: request.domain,
      kind: 'memory',
      op: 'create',
      supersedesId: null,
      sensitivity: 'normal',
      unifiedDiff: '+ extracted fact',
      newContent: '',
      provenance: `{"source":"document:${importId}"}`,
      gateReport: '{"checks":[],"passed":true}',
      requiresApproval: true,
      status: 'pending',
      createdAt: now,
      decidedAt: null,
      baseContentHash: null,
      importId,
    }
    const record: DocumentImportRecord = {
      id: importId,
      domain: request.domain,
      title: request.title,
      inputKind: request.inputKind,
      sourceRef: request.sourceUrl ?? (request.fileName ? `file:${request.fileName}` : 'manual:pasted-text'),
      sourcePath: `_sources/${request.domain}/${Date.now()}.md`,
      originalPath: request.mimeType === 'application/pdf'
        ? `_sources/${request.domain}/${Date.now()}.pdf`
        : null,
      contentHash: 'mock-hash',
      byteCount: request.contentEncoding === 'base64'
        ? Math.floor((request.content?.length ?? 0) * 3 / 4)
        : new TextEncoder().encode(request.content ?? '').length,
      candidateCount: 1,
      warningCount: 0,
      warnings: [],
      extractionEngine: request.mimeType === 'application/pdf'
        ? 'markitdown'
        : request.mimeType === 'message/rfc822'
          ? 'mail-parser'
          : request.mimeType === 'application/vnd.ms-outlook'
            ? 'msg-parser'
            : null,
      extractionVersion: request.mimeType === 'application/pdf' ? '0.1.6' : null,
      extractionQualityScore: request.mimeType === 'application/pdf' ? 96 : null,
      extractionQualityStatus: request.mimeType === 'application/pdf' ? 'passed' : 'not_applicable',
      extractionQualityIssues: [],
      status: 'pending',
      createdAt: now,
      updatedAt: now,
    }
    mockProposals = [proposal, ...mockProposals]
    mockDocumentImports = [record, ...mockDocumentImports]
    return {
      import: cloneMockValue(record),
      proposals: [cloneMockValue(proposal)],
      rejected: [],
      warnings: [],
    }
  }
  const payload = await invoke<DocumentImportResult>('memory_import_document', { request })
  return documentImportResultSchema.parse(payload)
}

export async function memoryDocumentImportsList(
  domain?: string,
): Promise<DocumentImportRecord[]> {
  if (!isTauriRuntime()) {
    return cloneMockValue(
      domain
        ? mockDocumentImports.filter((record) => record.domain === domain)
        : mockDocumentImports,
    )
  }
  const payload = await invoke<DocumentImportRecord[]>('memory_document_imports_list', {
    domain: domain ?? null,
  })
  return documentImportRecordSchema.array().parse(payload)
}

export async function memoryDocumentSourceRead(
  id: string,
): Promise<DocumentSourceReadResult> {
  if (!isTauriRuntime()) {
    throw new Error(`Source ${id} is only available in the desktop app.`)
  }
  const payload = await invoke<DocumentSourceReadResult>('memory_document_source_read', { id })
  return documentSourceReadResultSchema.parse(payload)
}

export async function memoryProposalsDecide(
  request: ProposalDecideRequest,
): Promise<MemoryWriteProposal> {
  if (!isTauriRuntime()) {
    const nextStatus = request.decision === 'approve' ? 'approved' : 'discarded'
    const updated = mockProposals.find((proposal) => proposal.id === request.id)
    if (!updated) throw new Error(`Unknown proposal ${request.id}`)
    updated.status = nextStatus
    updated.decidedAt = new Date().toISOString()
    return cloneMockValue(updated)
  }
  const payload = await invoke<MemoryWriteProposal>('memory_proposals_decide', { request })
  return memoryWriteProposalSchema.parse(payload)
}

export async function memoryConfirm(id: string): Promise<void> {
  if (!isTauriRuntime()) return
  await invoke('memory_confirm', { id })
}

export async function memoryLint(
  domain?: string,
  deep?: boolean,
): Promise<MemoryLintReport> {
  if (!isTauriRuntime()) {
    await new Promise((resolve) => setTimeout(resolve, 600))
    return {
      generatedAt: new Date().toISOString(),
      scanned: 2,
      findings: [
        {
          kind: 'orphan',
          severity: 'info',
          paths: ['work/meetings/2026-07-18-databricks-sync.md'],
          detail: "'Databricks sync' has no links in either direction and has never been retrieved.",
        },
        ...(deep
          ? [{
              kind: 'contradiction' as const,
              severity: 'warning' as const,
              paths: [
                'work/decisions/2026-07-20-powerreviews-feed-delta.md',
                'work/meetings/2026-07-18-databricks-sync.md',
              ],
              detail: 'One note says the feed is delta-only while the other assumes full loads.',
            }]
          : []),
      ],
      deep: deep ?? false,
      modelTokens: deep ? 812 : null,
    }
  }
  const payload = await invoke<MemoryLintReport>('memory_lint', {
    domain: domain ?? null,
    deep: deep ?? false,
  })
  return memoryLintReportSchema.parse(payload)
}

export async function memoryReindex(): Promise<ReindexResult> {
  if (!isTauriRuntime()) {
    return { indexed: 0, drifted: 0, orphaned: 0 }
  }
  const payload = await invoke<ReindexResult>('memory_reindex')
  return reindexResultSchema.parse(payload)
}

export async function memoryMaintenanceRun(): Promise<MaintenanceResult> {
  if (!isTauriRuntime()) {
    return { expired: 0, markedStale: 0, consolidationProposals: 0, deferredExpirations: 0 }
  }
  const payload = await invoke<MaintenanceResult>('memory_maintenance_run')
  return maintenanceResultSchema.parse(payload)
}

export async function memoryOperationsList(): Promise<MemoryOperationRecord[]> {
  if (!isTauriRuntime()) return []
  const payload = await invoke<MemoryOperationRecord[]>('memory_operations_list')
  return memoryOperationRecordSchema.array().parse(payload)
}

export async function memoryRetrievalBenchmark(): Promise<RetrievalBenchmarkReport> {
  if (!isTauriRuntime()) {
    return retrievalBenchmarkReportSchema.parse({
      generatedAt: new Date().toISOString(),
      corpusKind: 'browser-preview',
      cases: 0,
      corpusMemories: 0,
      baseline: { topOneAccuracy: 0, sourceHitRateAtFive: 0, sourceRecallAtFive: 0, meanReciprocalRank: 0, latencyP50Ms: 0, latencyP95Ms: 0, outboundCostUsd: 0 },
      candidate: { topOneAccuracy: 0, sourceHitRateAtFive: 0, sourceRecallAtFive: 0, meanReciprocalRank: 0, latencyP50Ms: 0, latencyP95Ms: 0, outboundCostUsd: 0 },
      production: { topOneAccuracy: 0, sourceHitRateAtFive: 0, sourceRecallAtFive: 0, meanReciprocalRank: 0, latencyP50Ms: 0, latencyP95Ms: 0, outboundCostUsd: 0 },
      fuzzyScanCount: 0,
      semanticBackend: 'not_configured',
      notes: ['Run the desktop app to benchmark the local corpus.'],
    })
  }
  const payload = await invoke<RetrievalBenchmarkReport>('memory_retrieval_benchmark')
  return retrievalBenchmarkReportSchema.parse(payload)
}

export async function memoryRetrievalEvalCasesList(): Promise<RetrievalEvalCase[]> {
  if (!isTauriRuntime()) return []
  const payload = await invoke<RetrievalEvalCase[]>('memory_retrieval_eval_cases_list')
  return retrievalEvalCaseSchema.array().parse(payload)
}

export async function memoryRetrievalEvalCaseSave(
  request: RetrievalEvalCaseRequest,
): Promise<RetrievalEvalCase> {
  const payload = await invoke<RetrievalEvalCase>('memory_retrieval_eval_case_save', { request })
  return retrievalEvalCaseSchema.parse(payload)
}

export async function memoryOrbitMap(
  domain?: string,
  includeSensitive = false,
  activityWindow: OrbitActivityWindow = 'today',
): Promise<OrbitMap> {
  if (!isTauriRuntime()) {
    return orbitMapSchema.parse({
      generatedAt: new Date().toISOString(),
      activityWindow,
      activities: [],
      counts: { skills: 0, memories: 0, routines: 0, applications: 0, relations: 0 },
      metrics: { composeMs: 0, tasksScanned: 0, tracesScanned: 0, activityEvents: 0 },
      nodes: [
        { id: 'core:agentic-os', kind: 'core', ring: 0, label: 'AgenticOS', subtitle: 'Local control plane', domain: null, sensitivity: null, status: 'active', operationalState: 'preview', catalogState: 'not_applicable', usageState: 'not_applicable', connectionState: 'not_applicable', domains: [], capabilities: [], lastActivityAt: null, sourcePath: null, sourceRef: 'runtime:agentic-os', groupId: null, count: 1, preview: 'Desktop data is loaded through Tauri.', updatedAt: null, actions: [], aggregate: false },
      ],
      edges: [],
    })
  }
  const payload = await invoke<OrbitMap>('memory_orbit_map', {
    domain: domain ?? null,
    includeSensitive,
    activityWindow,
  })
  return orbitMapSchema.parse(payload)
}

export async function skillsDistill(taskId: string): Promise<MemoryWriteProposal> {
  if (!isTauriRuntime()) {
    return {
      id: `proposal-mock-skill-${Date.now()}`,
      taskId,
      vaultPath: 'mock-skill/SKILL.md',
      domain: 'work',
      kind: 'skill',
      op: 'create',
      supersedesId: null,
      sensitivity: 'normal',
      unifiedDiff: '+++ b/mock-skill/SKILL.md',
      newContent: '# Mock skill',
      provenance: `{"source":"distill:${taskId}"}`,
      gateReport: '{"checks":[],"passed":true}',
      requiresApproval: true,
      status: 'pending',
      createdAt: new Date().toISOString(),
      decidedAt: null,
      baseContentHash: null,
      importId: null,
    }
  }
  const payload = await invoke<MemoryWriteProposal>('skills_distill', { taskId })
  return memoryWriteProposalSchema.parse(payload)
}
