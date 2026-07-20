import { useQuery } from '@tanstack/react-query'
import { getDashboardSnapshot } from '@/features/dashboard/api'

export const dashboardSnapshotQueryKey = ['dashboard-snapshot'] as const

export function useDashboardSnapshot() {
  return useQuery({
    queryKey: dashboardSnapshotQueryKey,
    queryFn: getDashboardSnapshot,
  })
}
