import { z } from 'zod'

export const domainSchema = z.enum([
  'work',
  'planphysique',
  'personal',
  'family',
  'finance',
  'research',
])

export const harnessSchema = z.enum(['codex', 'claude', 'acp'])

export const riskLevelSchema = z.enum(['low', 'medium', 'high', 'critical'])

export const originKindSchema = z.enum(['manual', 'workflow', 'schedule'])

export const taskStatusSchema = z.enum([
  'created',
  'classified',
  'planned',
  'running',
  'waiting_for_tool',
  'waiting_for_approval',
  'resuming',
  'verifying',
  'completed',
  'failed',
  'cancelled',
  'partially_completed',
])

export const taskSummarySchema = z.object({
  id: z.string(),
  title: z.string(),
  goal: z.string(),
  domain: domainSchema,
  agentId: z.string().nullable(),
  harness: harnessSchema,
  status: taskStatusSchema,
  originKind: originKindSchema,
  ontologyCategoryId: z.string().nullable(),
  currentStep: z.number(),
  stepCount: z.number(),
  costTokens: z.number(),
  costUsd: z.number().nullable(),
  pendingApprovalId: z.string().nullable(),
  riskLevel: riskLevelSchema,
  createdAt: z.string(),
  updatedAt: z.string(),
})

export const stepStatusSchema = z.enum(['pending', 'active', 'done', 'failed', 'skipped'])

export const taskStepSchema = z.object({
  index: z.number(),
  title: z.string(),
  status: stepStatusSchema,
})

export const artifactRefSchema = z.object({
  id: z.string(),
  label: z.string(),
  path: z.string(),
  kind: z.enum(['file', 'diff', 'report', 'draft']),
})

export const taskDetailSchema = taskSummarySchema.extend({
  planVersion: z.number(),
  steps: z.array(taskStepSchema),
  artifacts: z.array(artifactRefSchema),
  lastEventSeq: z.number(),
})

export const taskEventSchema = z.object({
  taskId: z.string(),
  seq: z.number(),
  ts: z.string(),
  kind: z.string(),
  payload: z.unknown(),
})

export const taskSubmitRequestSchema = z.object({
  goal: z.string(),
  domain: domainSchema.optional(),
  agentId: z.string().optional(),
  cwd: z.string().optional(),
})

export type Domain = z.infer<typeof domainSchema>
export type Harness = z.infer<typeof harnessSchema>
export type RiskLevel = z.infer<typeof riskLevelSchema>
export type TaskStatus = z.infer<typeof taskStatusSchema>
export type StepStatus = z.infer<typeof stepStatusSchema>
export type TaskSummary = z.infer<typeof taskSummarySchema>
export type TaskStep = z.infer<typeof taskStepSchema>
export type ArtifactRef = z.infer<typeof artifactRefSchema>
export type TaskDetail = z.infer<typeof taskDetailSchema>
export type TaskEvent = z.infer<typeof taskEventSchema>
export type TaskSubmitRequest = z.infer<typeof taskSubmitRequestSchema>
