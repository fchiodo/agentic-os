import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query'
import {
  memoryDocumentImportsList,
  memoryDocumentSourceRead,
  memoryImportDocument,
  memoryAsk,
  memoryConfirm,
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

export function useMemoryAsk() {
  return useMutation({
    mutationFn: (request: MemoryAskRequest) => memoryAsk(request),
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
