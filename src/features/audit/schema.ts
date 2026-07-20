import { z } from 'zod'

export const auditRunSummarySchema = z.object({
  runId: z.string(),
  taskId: z.string().nullable(),
  title: z.string(),
  ts: z.string(),
  status: z.string(),
  costUsd: z.number().nullable(),
})

export const traceEntrySchema = z.object({
  runId: z.string(),
  seq: z.number(),
  ts: z.string(),
  kind: z.string(),
  summary: z.string(),
  detail: z.unknown(),
  tokens: z.number().nullable(),
  costUsd: z.number().nullable(),
})

export const auditChainStatusSchema = z.object({
  ok: z.boolean(),
  checkedRows: z.number(),
  brokenAt: z.string().optional(),
})

export type AuditRunSummary = z.infer<typeof auditRunSummarySchema>
export type TraceEntry = z.infer<typeof traceEntrySchema>
export type AuditChainStatus = z.infer<typeof auditChainStatusSchema>
