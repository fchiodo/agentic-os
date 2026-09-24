import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ReactNode } from 'react'

const native = vi.hoisted(() => ({
  dragHandler: null as ((event: { payload: Record<string, unknown> }) => void) | null,
  dragUnlisten: vi.fn(),
  eventHandlers: new Map<string, (event: { payload: unknown }) => void>(),
  eventUnlisteners: [] as ReturnType<typeof vi.fn>[],
  inspectPaths: vi.fn(),
  listen: vi.fn(),
  onDragDropEvent: vi.fn(),
}))

vi.mock('@/lib/tauri', () => ({ isTauriRuntime: () => true }))
vi.mock('@tauri-apps/api/event', () => ({
  listen: (...args: unknown[]) => native.listen(...args),
}))
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: (...args: unknown[]) => native.onDragDropEvent(...args),
  }),
}))
vi.mock('./api', async (importOriginal) => ({
  ...await importOriginal<typeof import('./api')>(),
  inspectPaths: (...args: unknown[]) => native.inspectPaths(...args),
}))

import { DocumentConverterRuntimeBridge } from './runtime-bridge'
import { useConverterStore } from './store'

let queryClient: QueryClient

function wrapper({ children }: { children: ReactNode }) {
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
}

beforeEach(() => {
  queryClient = new QueryClient()
  native.dragHandler = null
  native.dragUnlisten.mockReset()
  native.eventHandlers.clear()
  native.eventUnlisteners = []
  native.inspectPaths.mockReset().mockResolvedValue([{
    errorCode: null,
    errorMessage: null,
    name: 'report.pdf',
    pageCount: 2,
    path: '/tmp/report.pdf',
    sizeBytes: 1_000,
    supported: true,
  }])
  native.listen.mockReset().mockImplementation(async (event: string, handler: (value: { payload: unknown }) => void) => {
    native.eventHandlers.set(event, handler)
    const unlisten = vi.fn()
    native.eventUnlisteners.push(unlisten)
    return unlisten
  })
  native.onDragDropEvent.mockReset().mockImplementation(async (handler: typeof native.dragHandler) => {
    native.dragHandler = handler
    return native.dragUnlisten
  })
  useConverterStore.setState({
    modelProgress: null,
    nativeDragActive: false,
    progressByJob: {},
    selected: [],
  })
})

afterEach(cleanup)

describe('DocumentConverterRuntimeBridge', () => {
  it('keeps one native subscription set while routes change repeatedly', async () => {
    const view = render(<DocumentConverterRuntimeBridge dropEnabled={false} />, { wrapper })

    await waitFor(() => {
      expect(native.listen).toHaveBeenCalledTimes(4)
      expect(native.onDragDropEvent).toHaveBeenCalledOnce()
    })

    for (let index = 0; index < 20; index += 1) {
      view.rerender(<DocumentConverterRuntimeBridge dropEnabled={index % 2 === 0} />)
    }

    expect(native.listen).toHaveBeenCalledTimes(4)
    expect(native.onDragDropEvent).toHaveBeenCalledOnce()

    view.rerender(<DocumentConverterRuntimeBridge dropEnabled />)
    act(() => native.dragHandler?.({ payload: { type: 'drop', paths: ['/tmp/report.pdf'] } }))
    await waitFor(() => expect(useConverterStore.getState().selected).toHaveLength(1))

    view.rerender(<DocumentConverterRuntimeBridge dropEnabled={false} />)
    act(() => native.dragHandler?.({ payload: { type: 'drop', paths: ['/tmp/ignored.pdf'] } }))
    expect(native.inspectPaths).toHaveBeenCalledTimes(1)

    view.unmount()
    expect(native.dragUnlisten).toHaveBeenCalledOnce()
    for (const unlisten of native.eventUnlisteners) expect(unlisten).toHaveBeenCalledOnce()
  })

  it('stores validated native progress without remounting the page', async () => {
    render(<DocumentConverterRuntimeBridge dropEnabled />, { wrapper })
    await waitFor(() => expect(native.listen).toHaveBeenCalledTimes(4))

    act(() => native.eventHandlers.get('document-converter:model-progress')?.({
      payload: {
        currentFile: null,
        downloadedBytes: 50,
        modelId: 'paddleocr-vl',
        percent: 50,
        stage: 'downloading',
        totalBytes: 100,
      },
    }))

    expect(useConverterStore.getState().modelProgress?.percent).toBe(50)
  })
})
