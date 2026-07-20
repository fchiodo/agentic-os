import { useQuery } from '@tanstack/react-query'
import { getControlStatus } from '@/features/control/api'

export const controlStatusQueryKey = ['control-status'] as const

export function useControlStatus() {
  return useQuery({
    queryKey: controlStatusQueryKey,
    queryFn: getControlStatus,
    refetchInterval: 5_000,
  })
}
