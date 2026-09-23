import '@testing-library/jest-dom/vitest'
import { act, cleanup, render, waitFor } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const sigmaMock = vi.hoisted(() => ({
  animate: vi.fn(),
  handlers: new Map<string, (payload: { edge?: string; node?: string }) => void>(),
  setState: vi.fn(),
}))

vi.mock('sigma', () => ({
  default: class SigmaMock {
    on(event: string, handler: (payload: { edge?: string; node?: string }) => void) {
      sigmaMock.handlers.set(event, handler)
    }

    kill() {}

    refresh() {}

    setSetting() {
      return this
    }

    getCamera() {
      return {
        animate: sigmaMock.animate,
        getState: () => ({ angle: 0, ratio: 1, x: 0.5, y: 0.5 }),
        setState: sigmaMock.setState,
      }
    }

    getNodeDisplayData(node: string) {
      return node === 'group:skill:test'
        ? { x: 0.23, y: 0.71 }
        : { x: 0.5, y: 0.5 }
    }
  },
}))

let generatedAt = '2026-09-23T10:00:00Z'

vi.mock('@/features/memory/hooks', () => ({
  useMemoryConfirm: () => ({ isPending: false, mutate: vi.fn() }),
  useMemoryOrbitMap: () => ({
    data: {
      generatedAt,
      nodes: [
        {
          id: 'core:agentic-os', kind: 'core', ring: 0, label: 'AgenticOS', subtitle: null,
          domain: null, sensitivity: null, status: 'active', operationalState: 'ready',
          catalogState: 'not_applicable', usageState: 'not_applicable', connectionState: 'not_applicable',
          domains: [], capabilities: [], lastActivityAt: null, sourcePath: null,
          sourceRef: 'runtime:agentic-os', groupId: null, count: 1, preview: null,
          updatedAt: null, actions: [], aggregate: false,
        },
        {
          id: 'group:skill:test', kind: 'skill_group', ring: 1, label: 'Test skills', subtitle: null,
          domain: null, sensitivity: null, status: 'active', operationalState: 'available',
          catalogState: 'registered', usageState: 'not_observed', connectionState: 'not_applicable',
          domains: [], capabilities: [], lastActivityAt: null, sourcePath: null,
          sourceRef: 'registry-group:skill:test', groupId: null, count: 1, preview: null,
          updatedAt: null, actions: ['expand'], aggregate: true,
        },
      ],
      edges: [{
        id: 'edge:registers', source: 'core:agentic-os', target: 'group:skill:test',
        relation: 'registers', evidence: 'declared', weight: 1, activityAt: null,
        provenance: [],
      }],
      counts: { skills: 1, memories: 0, routines: 0, applications: 0, relations: 1 },
      metrics: { composeMs: 1, tasksScanned: 0, tracesScanned: 0 },
    },
    error: null,
    isFetching: false,
    isLoading: false,
    refetch: vi.fn(),
  }),
}))

vi.mock('@/store/task-events', () => ({
  useTaskEventsStore: () => null,
}))

import { OrbitMapView } from '@/features/memory/orbit-map'

beforeEach(() => {
  generatedAt = '2026-09-23T10:00:00Z'
  sigmaMock.animate.mockClear()
  sigmaMock.setState.mockClear()
  sigmaMock.handlers.clear()
  vi.stubGlobal('requestAnimationFrame', vi.fn(() => 1))
  vi.stubGlobal('cancelAnimationFrame', vi.fn())
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe('OrbitMapView camera focus', () => {
  it('uses normalized display coordinates and does not refocus on data refresh', async () => {
    const view = render(<OrbitMapView onOpenMemory={vi.fn()} />)
    const clickNode = sigmaMock.handlers.get('clickNode')
    expect(clickNode).toBeDefined()

    act(() => clickNode?.({ node: 'group:skill:test' }))
    await waitFor(() => expect(sigmaMock.animate).toHaveBeenCalledTimes(1))
    expect(sigmaMock.animate).toHaveBeenCalledWith(
      { ratio: 0.72, x: 0.23, y: 0.71 },
      { duration: 260 },
    )

    sigmaMock.animate.mockClear()
    generatedAt = '2026-09-23T10:01:00Z'
    view.rerender(<OrbitMapView onOpenMemory={vi.fn()} />)
    await act(async () => {})
    expect(sigmaMock.animate).not.toHaveBeenCalled()
  })
})
