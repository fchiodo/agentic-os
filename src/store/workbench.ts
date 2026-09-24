import { create } from 'zustand'
import { persist } from 'zustand/middleware'
import type { CatalogKind } from '@/features/dashboard/schema'

type CatalogFilter = 'all' | CatalogKind

type WorkbenchState = {
  catalogFilter: CatalogFilter
  catalogSearch: string
  selectedCatalogId: string | null
  setCatalogFilter: (filter: CatalogFilter) => void
  setCatalogSearch: (value: string) => void
  setSelectedCatalogId: (value: string) => void
}

export const useWorkbenchStore = create<WorkbenchState>()(
  persist(
    (set) => ({
      catalogFilter: 'all',
      catalogSearch: '',
      selectedCatalogId: null,
      setCatalogFilter: (catalogFilter) => set({ catalogFilter }),
      setCatalogSearch: (catalogSearch) => set({ catalogSearch }),
      setSelectedCatalogId: (selectedCatalogId) => set({ selectedCatalogId }),
    }),
    {
      name: 'agent-control.workbench',
      partialize: (state) => ({
        catalogFilter: state.catalogFilter,
        catalogSearch: state.catalogSearch,
        selectedCatalogId: state.selectedCatalogId,
      }),
    },
  ),
)
