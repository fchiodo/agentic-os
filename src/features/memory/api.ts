import { invoke } from '@tauri-apps/api/core'
import {
  documentImportRecordSchema,
  documentImportResultSchema,
  documentSourceReadResultSchema,
  maintenanceResultSchema,
  memoryAnswerSchema,
  memoryIngestResultSchema,
  memoryReadResultSchema,
  memoryWriteProposalSchema,
  reindexResultSchema,
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
  type MemoryAskRequest,
  type MemoryIngestRequest,
  type MemoryIngestResult,
  type MemoryReadResult,
  type MemorySearchOpts,
  type MemoryWriteProposal,
  type ProposalDecideRequest,
  type ReindexResult,
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
    confidence: 0.9,
    sensitivity: 'normal',
    validFrom: null,
    validUntil: null,
    staleAfterDays: null,
    lastConfirmed: '2026-07-20T09:12:00Z',
    confirmations: 1,
    expires: null,
    tags: ['powerreviews', 'voc', 'sftp'],
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

export async function memoryAsk(request: MemoryAskRequest): Promise<MemoryAnswer> {
  if (!isTauriRuntime()) {
    return {
      id: '00000000-0000-4000-8000-000000000001',
      question: request.question,
      domain: request.domain,
      answer:
        'Delta feed daily instead of full: full files over 2GB hit the SFTP timeout. [1]',
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
  const payload = await invoke<MemoryAnswer>('memory_ask', { request })
  return memoryAnswerSchema.parse(payload)
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
    return {
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
  }
  const payload = await invoke<MemoryWriteProposal>('memory_save_manual', { request })
  return memoryWriteProposalSchema.parse(payload)
}

export async function memoryProposalsList(
  status?: string,
): Promise<MemoryWriteProposal[]> {
  if (!isTauriRuntime()) {
    return []
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
      extractionEngine: request.mimeType === 'application/pdf' ? 'markitdown' : null,
      extractionVersion: request.mimeType === 'application/pdf' ? '0.1.6' : null,
      extractionQualityScore: request.mimeType === 'application/pdf' ? 96 : null,
      extractionQualityStatus: request.mimeType === 'application/pdf' ? 'passed' : 'not_applicable',
      extractionQualityIssues: [],
      status: 'pending',
      createdAt: now,
      updatedAt: now,
    }
    return { import: record, proposals: [proposal], rejected: [], warnings: [] }
  }
  const payload = await invoke<DocumentImportResult>('memory_import_document', { request })
  return documentImportResultSchema.parse(payload)
}

export async function memoryDocumentImportsList(
  domain?: string,
): Promise<DocumentImportRecord[]> {
  if (!isTauriRuntime()) return []
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
  const payload = await invoke<MemoryWriteProposal>('memory_proposals_decide', { request })
  return memoryWriteProposalSchema.parse(payload)
}

export async function memoryConfirm(id: string): Promise<void> {
  await invoke('memory_confirm', { id })
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
    return { expired: 0, markedStale: 0 }
  }
  const payload = await invoke<MaintenanceResult>('memory_maintenance_run')
  return maintenanceResultSchema.parse(payload)
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
