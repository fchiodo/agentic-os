import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { CatalogKind } from '@/features/dashboard/schema'

type CatalogFilter = 'all' | CatalogKind
type RunnerMode = 'prompt' | 'routine'

type WorkbenchState = {
  catalogFilter: CatalogFilter
  catalogSearch: string
  draftPrompt: string
  runnerMode: RunnerMode
  selectedCatalogId: string | null
  selectedRoutineId: string | null
  setCatalogFilter: (filter: CatalogFilter) => void
  setCatalogSearch: (value: string) => void
  setDraftPrompt: (value: string) => void
  setRunnerMode: (mode: RunnerMode) => void
  setSelectedCatalogId: (value: string) => void
  setSelectedRoutineId: (value: string) => void
}

export const useWorkbenchStore = create<WorkbenchState>()(
  persist(
    (set) => ({
      catalogFilter: 'all',
      catalogSearch: '',
      draftPrompt: '',
      runnerMode: 'prompt',
      selectedCatalogId: null,
      selectedRoutineId: null,
      setCatalogFilter: (catalogFilter) => set({ catalogFilter }),
      setCatalogSearch: (catalogSearch) => set({ catalogSearch }),
      setDraftPrompt: (draftPrompt) => set({ draftPrompt }),
      setRunnerMode: (runnerMode) => set({ runnerMode }),
      setSelectedCatalogId: (selectedCatalogId) => set({ selectedCatalogId }),
      setSelectedRoutineId: (selectedRoutineId) => set({ selectedRoutineId }),
    }),
    {
      name: 'agent-control.workbench',
      partialize: (state) => ({
        catalogFilter: state.catalogFilter,
        catalogSearch: state.catalogSearch,
        draftPrompt: state.draftPrompt,
        runnerMode: state.runnerMode,
        selectedCatalogId: state.selectedCatalogId,
        selectedRoutineId: state.selectedRoutineId,
      }),
    },
  ),
)
