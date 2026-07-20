import { z } from 'zod'
import { domainSchema, riskLevelSchema } from '@/features/runner/schema'

export const previewBlockSchema = z.object({
  kind: z.enum(['diff', 'command', 'text']),
  content: z.string(),
})

export const approvalRequestSchema = z.object({
  id: z.string(),
  taskId: z.string(),
  taskTitle: z.string(),
  domain: domainSchema,
  toolName: z.string(),
  actionSummary: z.string(),
  riskLevel: riskLevelSchema,
  preview: previewBlockSchema.nullable(),
  requestedAt: z.string(),
})

export const approvalDecisionSchema = z.object({
  id: z.string(),
  decision: z.enum(['approve', 'deny']),
  note: z.string().optional(),
})

export type PreviewBlock = z.infer<typeof previewBlockSchema>
export type ApprovalRequest = z.infer<typeof approvalRequestSchema>
export type ApprovalDecisionInput = z.infer<typeof approvalDecisionSchema>
