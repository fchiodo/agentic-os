import { useQuery } from '@tanstack/react-query'
import { getAuditTrace, listAuditRuns, verifyAuditChain } from '@/features/audit/api'

export const auditRunsQueryKey = ['audit', 'runs'] as const
export const auditChainQueryKey = ['audit', 'chain'] as const
export const auditTraceQueryKey = (runId: string) => ['audit', 'trace', runId] as const

export function useAuditRuns() {
  return useQuery({
    queryKey: auditRunsQueryKey,
    queryFn: listAuditRuns,
  })
}

export function useAuditTrace(runId: string | null) {
  return useQuery({
    queryKey: runId ? auditTraceQueryKey(runId) : ['audit', 'trace', 'none'],
    queryFn: () => getAuditTrace(runId as string),
    enabled: Boolean(runId),
  })
}

export function useAuditChain() {
  return useQuery({
    queryKey: auditChainQueryKey,
    queryFn: verifyAuditChain,
  })
}
