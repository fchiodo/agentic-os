import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { decideApproval, listApprovals } from '@/features/approvals/api'
import type { ApprovalDecisionInput } from '@/features/approvals/schema'
import { controlStatusQueryKey } from '@/features/control/use-control-status'
import { tasksQueryKey } from '@/features/runner/hooks'

export const approvalsQueryKey = ['approvals'] as const

export function useApprovals() {
  return useQuery({
    queryKey: approvalsQueryKey,
    queryFn: listApprovals,
    refetchInterval: 5_000,
  })
}

export function useDecideApproval() {
  const queryClient = useQueryClient()

  return useMutation({
    mutationFn: (decision: ApprovalDecisionInput) => decideApproval(decision),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: approvalsQueryKey })
      void queryClient.invalidateQueries({ queryKey: tasksQueryKey })
      void queryClient.invalidateQueries({ queryKey: controlStatusQueryKey })
    },
  })
}
