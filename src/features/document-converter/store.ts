import { create } from 'zustand'
import type { ConversionOptions, SelectedDocument } from './schema'

type ConverterUiState = {
  selected: SelectedDocument[]
  destinationRoot: string | null
  options: ConversionOptions
  previewJobId: string | null
  setSelected: (selected: SelectedDocument[]) => void
  addSelected: (selected: SelectedDocument[]) => void
  removeSelected: (path: string) => void
  clearSelected: () => void
  setDestinationRoot: (path: string | null) => void
  setProcessingMode: (mode: ConversionOptions['processingMode']) => void
  setPreservePageImages: (preserve: boolean) => void
  setPreviewJobId: (id: string | null) => void
}

export const useConverterStore = create<ConverterUiState>((set) => ({
  selected: [],
  destinationRoot: null,
  options: {
    processingMode: 'automatic',
    maxTokensPerPage: 2048,
    preservePageImages: false,
  },
  previewJobId: null,
  setSelected: (selected) => set({ selected }),
  addSelected: (incoming) => set((state) => {
    const unique = new Map(state.selected.map((item) => [item.path, item]))
    for (const item of incoming) unique.set(item.path, item)
    return { selected: [...unique.values()] }
  }),
  removeSelected: (path) => set((state) => ({
    selected: state.selected.filter((item) => item.path !== path),
  })),
  clearSelected: () => set({ selected: [] }),
  setDestinationRoot: (destinationRoot) => set({ destinationRoot }),
  setProcessingMode: (processingMode) => set((state) => ({
    options: { ...state.options, processingMode },
  })),
  setPreservePageImages: (preservePageImages) => set((state) => ({
    options: { ...state.options, preservePageImages },
  })),
  setPreviewJobId: (previewJobId) => set({ previewJobId }),
}))
