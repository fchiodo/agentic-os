import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import { useRef, useState } from 'react'
import {
  memoryDocumentImportsList,
  memoryDocumentSourceRead,
  memoryImportDocument,
  memoryAsk,
  memoryAskCancel,
  memoryAnswerFeedback,
  memoryConfirm,
  memoryLint,
  memoryMaintenanceRun,
  memoryProposalsDecide,
  memoryProposalsList,
  memoryRead,
  memoryReindex,
  memorySaveManual,
  memorySearch,
  memoryTree,
  skillsDistill,
} from '@/features/memory/api'
import type {
  DocumentImportRequest,
  ManualSaveRequest,
  MemoryAnswerFeedbackRequest,
  MemoryAskProgress,
  MemoryAskRequest,
  ProposalDecideRequest,
} from '@/features/memory/schema'

export const memoryTreeQueryKey = ['memory', 'tree'] as const
export const memorySearchQueryKey = ['memory', 'search'] as const
export const memoryProposalsQueryKey = ['memory', 'proposals'] as const
export const memoryDocumentImportsQueryKey = ['memory', 'document-imports'] as const

export function useMemoryTree(domain?: string) {
  return useQuery({
    queryKey: [...memoryTreeQueryKey, domain],
    queryFn: () => memoryTree(domain),
  })
}

export function useMemoryRead(path: string | null) {
  return useQuery({
    queryKey: ['memory', 'read', path],
    queryFn: () => memoryRead(path!),
    enabled: path !== null,
  })
}

export function useMemorySearch(
  query: string,
  domain?: string,
  includeStale?: boolean,
) {
  return useQuery({
    queryKey: [...memorySearchQueryKey, query, domain, includeStale],
    queryFn: () =>
      memorySearch(query, domain, {
        includeStale: includeStale ?? true,
        limit: 8,
      }),
    enabled: query.trim().length >= 2,
  })
}

export function useMemoryProposals(status?: string) {
  return useQuery({
    queryKey: [...memoryProposalsQueryKey, status],
    queryFn: () => memoryProposalsList(status),
  })
}

/**
 * Ask mutation plus the live progress trail streamed over the per-invocation
 * Tauri channel. `progress` resets on each ask and is intentionally kept
 * after settling so the trail stays inspectable on errors (e.g. showing the
 * last stage reached before a failed synthesis). `stop` cancels the in-flight
 * run — there is no wall-clock timeout on the backend, the user decides.
 */
export function useMemoryAsk() {
  const [progress, setProgress] = useState<MemoryAskProgress[]>([])
  const [durationMs, setDurationMs] = useState<number | null>(null)
  const runRef = useRef(0)
  const askIdRef = useRef<string | null>(null)
  const startedAtRef = useRef<number | null>(null)

  const mutation = useMutation({
    mutationFn: (request: MemoryAskRequest) => {
      const run = ++runRef.current
      const askId = crypto.randomUUID()
      askIdRef.current = askId
      startedAtRef.current = Date.now()
      setProgress([])
      setDurationMs(null)
      return memoryAsk(request, askId, (event) => {
        // The run guard drops stragglers from a superseded ask so a rapid
        // re-submit can never interleave two progress trails.
        if (runRef.current === run) {
          setProgress((previous) => {
            // Transient events (heartbeats, stderr lines) update in place —
            // a stalled run shows one live status line, not a growing stack.
            const last = previous[previous.length - 1]
            if (last?.transient && event.transient) {
              return [...previous.slice(0, -1), event]
            }
            return [...previous, event]
          })
        }
      })
    },
    onSettled: () => {
      setDurationMs(startedAtRef.current === null ? null : Date.now() - startedAtRef.current)
    },
  })

  const stop = () => {
    if (askIdRef.current !== null) {
      void memoryAskCancel(askIdRef.current)
    }
  }

  return { ...mutation, progress, durationMs, stop }
}

export function useMemoryAnswerFeedback() {
  return useMutation({
    mutationFn: (request: MemoryAnswerFeedbackRequest) => memoryAnswerFeedback(request),
  })
}

export function useMemoryDocumentImports(domain?: string) {
  return useQuery({
    queryKey: [...memoryDocumentImportsQueryKey, domain],
    queryFn: () => memoryDocumentImportsList(domain),
  })
}

export function useMemoryDocumentSourceRead(id: string | null) {
  return useQuery({
    queryKey: ['memory', 'document-source', id],
    queryFn: () => memoryDocumentSourceRead(id!),
    enabled: id !== null,
  })
}

export function useMemoryImportDocument() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (request: DocumentImportRequest) => memoryImportDocument(request),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: memoryDocumentImportsQueryKey })
      void queryClient.invalidateQueries({ queryKey: memoryProposalsQueryKey })
    },
  })
}

export function useMemorySaveManual() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (request: ManualSaveRequest) => memorySaveManual(request),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: memoryTreeQueryKey })
      void queryClient.invalidateQueries({ queryKey: memorySearchQueryKey })
      void queryClient.invalidateQueries({ queryKey: memoryProposalsQueryKey })
      void queryClient.invalidateQueries({ queryKey: memoryDocumentImportsQueryKey })
    },
  })
}

export function useMemoryProposalsDecide() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (request: ProposalDecideRequest) => memoryProposalsDecide(request),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: memoryTreeQueryKey })
      void queryClient.invalidateQueries({ queryKey: memorySearchQueryKey })
      void queryClient.invalidateQueries({ queryKey: memoryProposalsQueryKey })
      void queryClient.invalidateQueries({ queryKey: memoryDocumentImportsQueryKey })
    },
  })
}

export function useMemoryConfirm() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (id: string) => memoryConfirm(id),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: memorySearchQueryKey })
      void queryClient.invalidateQueries({ queryKey: ['memory', 'read'] })
      void queryClient.invalidateQueries({ queryKey: memoryTreeQueryKey })
    },
  })
}

export function useMemoryReindex() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: memoryReindex,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: memoryTreeQueryKey })
      void queryClient.invalidateQueries({ queryKey: memorySearchQueryKey })
    },
  })
}

export function useMemoryLint() {
  return useMutation({
    mutationFn: ({ domain, deep }: { domain?: string; deep?: boolean }) =>
      memoryLint(domain, deep),
  })
}

export function useMemoryMaintenanceRun() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: memoryMaintenanceRun,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: memoryTreeQueryKey })
      void queryClient.invalidateQueries({ queryKey: memorySearchQueryKey })
    },
  })
}

export function useSkillsDistill() {
  const queryClient = useQueryClient()
  return useMutation({
    mutationFn: (taskId: string) => skillsDistill(taskId),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: memoryProposalsQueryKey })
    },
  })
}
