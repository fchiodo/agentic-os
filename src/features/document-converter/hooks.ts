import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import * as api from './api'

export const converterStatusKey = ['document-converter', 'status'] as const
export const converterJobsKey = ['document-converter', 'jobs'] as const

export function useConverterStatus() {
  return useQuery({
    queryKey: converterStatusKey,
    queryFn: api.getConverterStatus,
    refetchInterval: 5_000,
  })
}

export function useConversionJobs() {
  return useQuery({
    queryKey: converterJobsKey,
    queryFn: () => api.listJobs(),
    refetchInterval: (query) => query.state.data?.some((job) =>
      ['queued', 'preparing', 'rendering', 'ocr', 'reconstructing', 'writing'].includes(job.status),
    ) ? 1_000 : 10_000,
  })
}

export function useInstallModel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: api.installModel,
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: converterStatusKey }),
  })
}

export function useRepairModel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: api.repairModel,
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: converterStatusKey }),
  })
}

export function useRemoveModel() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: api.removeModel,
    onSuccess: async () => queryClient.invalidateQueries({ queryKey: converterStatusKey }),
  })
}
