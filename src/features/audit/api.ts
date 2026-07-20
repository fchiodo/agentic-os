import { invoke } from '@tauri-apps/api/core'
import { mockAuditRuns, mockTrace } from '@/features/audit/mock-data'
import {
  auditChainStatusSchema,
  auditRunSummarySchema,
  traceEntrySchema,
  type AuditChainStatus,
  type AuditRunSummary,
  type TraceEntry,
} from '@/features/audit/schema'
import { isTauriRuntime } from '@/lib/tauri'

export async function listAuditRuns(): Promise<AuditRunSummary[]> {
  const payload = isTauriRuntime()
    ? await invoke<AuditRunSummary[]>('audit_runs')
    : mockAuditRuns

  return auditRunSummarySchema.array().parse(payload)
}

export async function getAuditTrace(runId: string): Promise<TraceEntry[]> {
  const payload = isTauriRuntime()
    ? await invoke<TraceEntry[]>('audit_trace', { runId })
    : (mockTrace[runId] ?? [])

  return traceEntrySchema.array().parse(payload)
}

export async function verifyAuditChain(): Promise<AuditChainStatus> {
  const payload = isTauriRuntime()
    ? await invoke<AuditChainStatus>('audit_verify_chain')
    : { ok: true, checkedRows: mockAuditRuns.length }

  return auditChainStatusSchema.parse(payload)
}
