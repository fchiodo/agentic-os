import { z } from 'zod'

export const memoryTypeSchema = z.enum([
  'fact',
  'decision',
  'preference',
  'entity',
  'episode',
])

export const memoryStatusSchema = z.enum(['active', 'stale', 'expired'])

export const sensitivitySchema = z.enum(['normal', 'sensitive'])

export const provenanceSchema = z.object({
  source: z.string(),
  ts: z.string(),
})

export const memoryFrontmatterSchema = z.object({
  id: z.string(),
  memType: memoryTypeSchema,
  domain: z.string(),
  title: z.string(),
  created: z.string(),
  updated: z.string(),
  provenance: provenanceSchema,
  confidence: z.number(),
  sensitivity: sensitivitySchema,
  validFrom: z.string().nullable().optional(),
  validUntil: z.string().nullable().optional(),
  staleAfterDays: z.number().nullable().optional(),
  lastConfirmed: z.string().nullable().optional(),
  confirmations: z.number().nullable().optional(),
  expires: z.string().nullable().optional(),
  tags: z.array(z.string()),
})

export const memoryRowSchema = z.object({
  id: z.string(),
  vaultPath: z.string(),
  domain: z.string(),
  memType: memoryTypeSchema,
  title: z.string(),
  summary: z.string().nullable(),
  sensitivity: sensitivitySchema,
  confidence: z.number(),
  createdAt: z.string(),
  updatedAt: z.string(),
  validFrom: z.string().nullable(),
  validUntil: z.string().nullable(),
  staleAfterDays: z.number().nullable(),
  lastConfirmedAt: z.string().nullable(),
  confirmationCount: z.number(),
  lastAccessedAt: z.string().nullable(),
  accessCount: z.number(),
  expiresAt: z.string().nullable(),
  provenance: z.string(),
  contentHash: z.string(),
  status: memoryStatusSchema,
})

export const scoredMemorySchema = z.object({
  row: memoryRowSchema,
  score: z.number(),
  relevance: z.number(),
  recency: z.number(),
  trust: z.number(),
})

export interface VaultNode {
  name: string
  path: string
  isDir: boolean
  children: VaultNode[]
  memoryId?: string | null
  memType?: string | null
  status?: string | null
}

export const vaultNodeSchema: z.ZodType<VaultNode> = z.lazy(() =>
  z.object({
    name: z.string(),
    path: z.string(),
    isDir: z.boolean(),
    children: z.array(vaultNodeSchema),
    memoryId: z.string().nullable(),
    memType: z.string().nullable(),
    status: z.string().nullable(),
  })
)

export const memoryReadResultSchema = z.object({
  frontmatter: memoryFrontmatterSchema.nullable(),
  markdown: z.string(),
  status: z.string(),
  gitLastCommit: z.string().nullable(),
})

export const proposalOpSchema = z.enum(['create', 'update', 'supersede'])

export const proposalStatusSchema = z.enum([
  'pending',
  'approved',
  'discarded',
  'auto_applied',
])

export const memoryWriteProposalSchema = z.object({
  id: z.string(),
  taskId: z.string().nullable(),
  vaultPath: z.string(),
  domain: z.string(),
  kind: z.string(),
  op: proposalOpSchema,
  supersedesId: z.string().nullable(),
  sensitivity: sensitivitySchema,
  unifiedDiff: z.string(),
  newContent: z.string(),
  provenance: z.string(),
  gateReport: z.string(),
  requiresApproval: z.boolean(),
  status: proposalStatusSchema,
  createdAt: z.string(),
  decidedAt: z.string().nullable(),
})

export const reindexResultSchema = z.object({
  indexed: z.number(),
  drifted: z.number(),
  orphaned: z.number(),
})

export const maintenanceResultSchema = z.object({
  expired: z.number(),
  markedStale: z.number(),
})

export const manualSaveRequestSchema = z.object({
  domain: z.string(),
  memType: z.string(),
  title: z.string(),
  body: z.string(),
  tags: z.array(z.string()),
})

export const proposalDecideRequestSchema = z.object({
  id: z.string(),
  decision: z.string(),
})

export const memorySearchOptsSchema = z.object({
  includeStale: z.boolean(),
  limit: z.number().optional(),
})

export type MemoryType = z.infer<typeof memoryTypeSchema>
export type MemoryStatus = z.infer<typeof memoryStatusSchema>
export type Sensitivity = z.infer<typeof sensitivitySchema>
export type Provenance = z.infer<typeof provenanceSchema>
export type MemoryFrontmatter = z.infer<typeof memoryFrontmatterSchema>
export type MemoryRow = z.infer<typeof memoryRowSchema>
export type ScoredMemory = z.infer<typeof scoredMemorySchema>
// VaultNode is defined as an interface above to avoid circular inference
export type MemoryReadResult = z.infer<typeof memoryReadResultSchema>
export type ProposalOp = z.infer<typeof proposalOpSchema>
export type ProposalStatus = z.infer<typeof proposalStatusSchema>
export type MemoryWriteProposal = z.infer<typeof memoryWriteProposalSchema>
export type ReindexResult = z.infer<typeof reindexResultSchema>
export type MaintenanceResult = z.infer<typeof maintenanceResultSchema>
export type ManualSaveRequest = z.infer<typeof manualSaveRequestSchema>
export type ProposalDecideRequest = z.infer<typeof proposalDecideRequestSchema>
export type MemorySearchOpts = z.infer<typeof memorySearchOptsSchema>
