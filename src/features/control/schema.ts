import { z } from 'zod'

export const controlStatusSchema = z.object({
  pendingMemoryProposals: z.number(),
})

export type ControlStatus = z.infer<typeof controlStatusSchema>
