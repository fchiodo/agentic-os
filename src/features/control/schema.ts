import { z } from 'zod'

export const controlStatusSchema = z.object({
  pendingApprovals: z.number(),
  pendingMemoryProposals: z.number(),
  runningTasks: z.number(),
  spentTodayUsd: z.number(),
  auditChainOk: z.boolean(),
})

export type ControlStatus = z.infer<typeof controlStatusSchema>
