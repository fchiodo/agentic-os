import { listen } from '@tauri-apps/api/event'
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useEffect } from 'react'
import { isTauriRuntime } from '@/lib/tauri'
import * as api from './api'
import {
  conversionProgressSchema,
  modelProgressSchema,
  type ConversionProgress,
  type ModelProgress,
} from './schema'

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

export function useConverterEvents(
  onModelProgress: (progress: ModelProgress) => void,
  onConversionProgress: (progress: ConversionProgress) => void,
) {
  const queryClient = useQueryClient()
  useEffect(() => {
    if (!isTauriRuntime()) {
      return
    }
    const unlisten = Promise.all([
      listen('document-converter:model-progress', (event) => {
        const parsed = modelProgressSchema.safeParse(event.payload)
        if (parsed.success) onModelProgress(parsed.data)
      }),
      listen('document-converter:conversion-progress', (event) => {
        const parsed = conversionProgressSchema.safeParse(event.payload)
        if (parsed.success) onConversionProgress(parsed.data)
        void queryClient.invalidateQueries({ queryKey: converterJobsKey })
      }),
      listen('document-converter:conversion-completed', () => {
        void queryClient.invalidateQueries({ queryKey: converterJobsKey })
      }),
      listen('document-converter:conversion-failed', () => {
        void queryClient.invalidateQueries({ queryKey: converterJobsKey })
      }),
    ])
    return () => {
      void unlisten.then((callbacks) => callbacks.forEach((callback) => callback()))
    }
  }, [onConversionProgress, onModelProgress, queryClient])
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
