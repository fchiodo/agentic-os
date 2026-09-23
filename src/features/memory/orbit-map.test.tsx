import '@testing-library/jest-dom/vitest'
import type Graph from 'graphology'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { orbitMapSchema } from '@/features/memory/schema'
import type { OrbitMap, OrbitNode } from '@/features/memory/schema'

const sigmaMock = vi.hoisted(() => ({
  animate: vi.fn(),
  graph: null as Graph | null,
  handlers: new Map<string, (payload: { edge?: string; node?: string }) => void>(),
  setState: vi.fn(),
}))

const orbitHookState = vi.hoisted(() => ({
  current: null as OrbitMap | null,
  refetch: vi.fn(),
}))

vi.mock('sigma', () => ({
  default: class SigmaMock {
    constructor(graph: Graph) {
      sigmaMock.graph = graph
    }

    on(event: string, handler: (payload: { edge?: string; node?: string }) => void) {
      sigmaMock.handlers.set(event, handler)
    }

    kill() {}
    refresh() {}
    setSetting() { return this }

    getCamera() {
      return {
        animate: sigmaMock.animate,
        getState: () => ({ angle: 0, ratio: 1, x: 0.5, y: 0.5 }),
        setState: sigmaMock.setState,
      }
    }

    getNodeDisplayData(node: string) {
      if (node === 'skill:deep') return { x: 0.18, y: 0.76 }
      if (node === 'group:skill:test') return { x: 0.23, y: 0.71 }
      return { x: 0.5, y: 0.5 }
    }
  },
}))

vi.mock('sigma/rendering', () => ({ EdgeArrowProgram: class {}, EdgeLineProgram: class {} }))

vi.mock('@/features/memory/hooks', () => ({
  useMemoryConfirm: () => ({ isPending: false, mutate: vi.fn() }),
  useMemoryOrbitMap: () => ({
    data: orbitHookState.current,
    error: null,
    isFetching: false,
    isLoading: false,
    refetch: orbitHookState.refetch,
  }),
}))

vi.mock('@/store/task-events', () => ({ useTaskEventsStore: () => null }))

import { radialPositions } from '@/features/memory/orbit-layout'
import { OrbitMapView } from '@/features/memory/orbit-map'

function node(overrides: Partial<OrbitNode> & Pick<OrbitNode, 'id' | 'kind' | 'ring' | 'label'>): OrbitNode {
  return {
    subtitle: null,
    domain: null,
    sensitivity: null,
    status: 'active',
    operationalState: 'available',
    catalogState: 'registered',
    usageState: 'not_observed',
    connectionState: 'not_applicable',
    domains: [],
    capabilities: [],
    lastActivityAt: null,
    sourcePath: null,
    sourceRef: `fixture:${overrides.id}`,
    groupId: null,
    count: 1,
    preview: null,
    updatedAt: null,
    actions: [],
    aggregate: false,
    ...overrides,
  }
}

function fixture(generatedAt = '2026-09-23T10:00:00Z'): OrbitMap {
  const nodes: OrbitNode[] = [
    node({ id: 'core:agentic-os', kind: 'core', ring: 0, label: 'AgenticOS', operationalState: 'ready', catalogState: 'not_applicable', sourceRef: 'runtime:agentic-os' }),
    node({ id: 'group:skill:test', kind: 'skill_group', ring: 1, label: 'Test skills', aggregate: true, count: 1, actions: ['expand'] }),
    node({ id: 'skill:deep', kind: 'skill', ring: 1, label: 'Deep skill', groupId: 'group:skill:test', actions: ['open_catalog'] }),
    ...(['work', 'planphysique', 'personal', 'family', 'finance', 'research'] as const).map((domain) => node({
      id: `group:memory:${domain}`,
      kind: 'memory_domain',
      ring: 2,
      label: domain === 'planphysique' ? 'PlanPhysique' : domain[0].toUpperCase() + domain.slice(1),
      domain,
      aggregate: true,
      count: domain === 'work' ? 1 : 0,
      actions: ['expand'],
    })),
    node({ id: 'memory:decision', kind: 'memory', ring: 2, label: 'Validated decision', domain: 'work', groupId: 'group:memory:work', sourcePath: 'work/decisions/validated.md', actions: ['open_memory'] }),
    node({ id: 'group:routine:empty', kind: 'routine_group', ring: 3, label: 'Routines', aggregate: true, count: 0, actions: ['expand'] }),
    node({ id: 'group:application:mcp', kind: 'application_group', ring: 4, label: 'MCP applications', aggregate: true, count: 1, actions: ['expand'] }),
    node({ id: 'application:github', kind: 'application', ring: 4, label: 'GitHub', groupId: 'group:application:mcp', connectionState: 'working', usageState: 'observed' }),
  ]
  return {
    generatedAt,
    activityWindow: 'today',
    nodes,
    edges: [
      { id: 'edge:registers', source: 'core:agentic-os', target: 'group:skill:test', relation: 'registers', evidence: 'declared', weight: 1, activityAt: null, provenance: [{ kind: 'registry', reference: 'catalog:skill:test', detail: 'Registered skill group', ts: null }] },
      { id: 'edge:contains', source: 'group:skill:test', target: 'skill:deep', relation: 'contains', evidence: 'declared', weight: 1, activityAt: null, provenance: [{ kind: 'registry', reference: 'catalog:deep', detail: 'Discovered skill', ts: null }] },
      { id: 'edge:observed', source: 'skill:deep', target: 'application:github', relation: 'used', evidence: 'observed', weight: 1, activityAt: '2026-09-23T09:00:00Z', provenance: [{ kind: 'execution_event', reference: 'audit:run-1', detail: 'Successful structured invocation', ts: '2026-09-23T09:00:00Z' }] },
    ],
    activities: [
      { taskId: 'task-1', title: 'Run governed task', domain: 'work', status: 'completed', updatedAt: '2026-09-23T09:00:00Z', eventCount: 2, telemetryAvailable: true, links: [
        { nodeId: 'skill:deep', relation: 'used', eventRef: 'audit:run-1', detail: 'Structured skill reference', occurredAt: '2026-09-23T09:00:00Z', outcome: 'success' },
        { nodeId: 'memory:decision', relation: 'inserted_into_context', eventRef: 'audit:run-1', detail: 'Structured memory context reference', occurredAt: '2026-09-23T09:00:00Z', outcome: null },
      ] },
      { taskId: 'task-2', title: 'Task without trace', domain: 'personal', status: 'queued', updatedAt: '2026-09-23T08:00:00Z', eventCount: 0, telemetryAvailable: false, links: [] },
    ],
    counts: { skills: 1, memories: 1, routines: 0, applications: 1, relations: 3 },
    metrics: { composeMs: 1.2, tasksScanned: 2, tracesScanned: 1, activityEvents: 2 },
  }
}

function renderMap() {
  return render(<MemoryRouter><OrbitMapView onOpenMemory={vi.fn()} /></MemoryRouter>)
}

beforeEach(() => {
  orbitHookState.current = fixture()
  orbitHookState.refetch.mockReset()
  sigmaMock.animate.mockReset()
  sigmaMock.setState.mockReset()
  sigmaMock.handlers.clear()
  sigmaMock.graph = null
  vi.stubGlobal('matchMedia', vi.fn(() => ({
    matches: true,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
  })))
  vi.stubGlobal('requestAnimationFrame', vi.fn((callback: FrameRequestCallback) => {
    callback(performance.now())
    return 1
  }))
  vi.stubGlobal('cancelAnimationFrame', vi.fn())
})

afterEach(() => {
  cleanup()
  vi.unstubAllGlobals()
})

describe('OrbitMapView exploration', () => {
  it('validates the Rust/TypeScript activity payload boundary', () => {
    expect(orbitMapSchema.parse(fixture()).activities[0].links[0].eventRef).toBe('audit:run-1')
  })

  it('keeps selection separate from expansion and only focuses on request', async () => {
    renderMap()
    const clickNode = sigmaMock.handlers.get('clickNode')
    expect(clickNode).toBeDefined()

    act(() => clickNode?.({ node: 'group:skill:test' }))
    expect(await screen.findByRole('heading', { name: 'Test skills' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Expand group' })).toBeInTheDocument()
    expect(sigmaMock.animate).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Center selection' }))
    expect(sigmaMock.setState).toHaveBeenCalledWith({ ratio: 0.58, x: 0.23, y: 0.71 })
  })

  it('renders memory sector separators through transparent visible anchors', () => {
    renderMap()
    const graph = sigmaMock.graph
    expect(graph).not.toBeNull()
    for (let index = 0; index < 6; index += 1) {
      expect(graph?.hasEdge(`__sector-edge:${index}`)).toBe(true)
      expect(graph?.getNodeAttribute(`__sector:${index}:inner`, 'hidden')).not.toBe(true)
      expect(graph?.getNodeAttribute(`__sector:${index}:outer`, 'hidden')).not.toBe(true)
      expect(graph?.getNodeAttribute(`__sector:${index}:inner`, 'color')).toBe('#ffffff00')
    }
  })

  it('finds an item in a closed group, expands its parent, and opens the detail', async () => {
    renderMap()
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search all map items' }), { target: { value: 'Deep skill' } })
    expect(screen.getByText((_content, element) => element?.textContent === '1 result in authorized data')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('option', { name: /Deep skill/ }))

    expect(await screen.findByRole('heading', { name: 'Deep skill' })).toBeInTheDocument()
    expect(sigmaMock.setState).toHaveBeenCalledWith({ ratio: 0.58, x: 0.18, y: 0.76 })
    fireEvent.click(screen.getByText('Explore as an accessible list'))
    expect(screen.getByRole('button', { name: /Deep skill/ })).toBeInTheDocument()
  })

  it('does not move the camera on a payload refresh and closes selection with Escape', async () => {
    const view = renderMap()
    act(() => sigmaMock.handlers.get('clickNode')?.({ node: 'group:skill:test' }))
    fireEvent.click(await screen.findByRole('button', { name: 'Center selection' }))
    sigmaMock.setState.mockClear()

    orbitHookState.current = fixture('2026-09-23T10:01:00Z')
    view.rerender(<MemoryRouter><OrbitMapView onOpenMemory={vi.fn()} /></MemoryRouter>)
    await act(async () => {})
    expect(sigmaMock.setState).not.toHaveBeenCalledWith(expect.objectContaining({ ratio: 0.58 }))

    fireEvent.keyDown(window, { key: 'Escape' })
    await waitFor(() => expect(screen.getByRole('heading', { name: 'System overview' })).toBeInTheDocument())
  })

  it('separates recorded activity from unavailable telemetry', async () => {
    renderMap()
    fireEvent.click(screen.getByRole('button', { name: 'Activity' }))
    fireEvent.click(screen.getByRole('button', { name: /Run governed task/ }))
    expect(await screen.findByRole('heading', { name: 'Run governed task' })).toBeInTheDocument()
    expect(screen.getByText('2 recorded events')).toBeInTheDocument()
    expect(screen.getByText(/not claimed to have determined the answer/i)).toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Task without trace/ }))
    expect(await screen.findByText(/Data not available: this task has no trace events/i)).toBeInTheDocument()
  })

  it('keeps the selected task while inspecting one of its linked nodes', async () => {
    renderMap()
    fireEvent.click(screen.getByRole('button', { name: 'Activity' }))
    const taskButton = screen.getByRole('button', { name: /Run governed task/ })
    fireEvent.click(taskButton)
    act(() => sigmaMock.handlers.get('clickNode')?.({ node: 'skill:deep' }))

    expect(await screen.findByRole('heading', { name: 'Deep skill' })).toBeInTheDocument()
    expect(screen.getByText('Inspecting within task: Run governed task')).toBeInTheDocument()
    expect(taskButton).toHaveAttribute('aria-pressed', 'true')
    expect(sigmaMock.graph?.hasEdge('activity:task-1:0')).toBe(true)
  })

  it('applies evidence filters to selected-node and activity relations', async () => {
    renderMap()
    act(() => sigmaMock.handlers.get('clickNode')?.({ node: 'skill:deep' }))
    expect(sigmaMock.graph?.hasEdge('edge:observed')).toBe(true)

    fireEvent.click(screen.getByRole('button', { name: 'Show relations' }))
    fireEvent.click(screen.getByRole('checkbox', { name: 'Observed' }))
    await waitFor(() => expect(sigmaMock.graph?.hasEdge('edge:observed')).toBe(false))

    fireEvent.click(screen.getByRole('button', { name: 'Activity' }))
    fireEvent.click(screen.getByRole('button', { name: /Run governed task/ }))
    expect(sigmaMock.graph?.hasEdge('activity:task-1:0')).toBe(false)
  })

  it('routes registry evidence to Catalog instead of presenting it as a trace', async () => {
    renderMap()
    act(() => sigmaMock.handlers.get('clickNode')?.({ node: 'skill:deep' }))
    fireEvent.click(await screen.findByRole('button', { name: /target of contains/i }))

    expect(screen.getByRole('button', { name: 'Open in Catalog · catalog:deep' })).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: /Open trace/ })).not.toBeInTheDocument()
  })

  it('reports the complete search count and progressively reveals results after forty', () => {
    const current = fixture()
    const matchingNodes = Array.from({ length: 45 }, (_, index) => node({
      id: `skill:matching-${index.toString().padStart(2, '0')}`,
      kind: 'skill',
      ring: 1,
      label: `Matching skill ${index + 1}`,
      groupId: 'group:skill:test',
    }))
    orbitHookState.current = { ...current, nodes: [...current.nodes, ...matchingNodes] }
    renderMap()

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search all map items' }), { target: { value: 'Matching skill' } })
    expect(screen.getByText((_content, element) => element?.textContent === '45 results in authorized data · showing 40')).toBeInTheDocument()
    expect(within(screen.getByRole('listbox', { name: 'Map search results' })).getAllByRole('option')).toHaveLength(40)
    fireEvent.click(screen.getByRole('button', { name: 'Show 5 more' }))
    expect(within(screen.getByRole('listbox', { name: 'Map search results' })).getAllByRole('option')).toHaveLength(45)
  })
})

describe('radialPositions', () => {
  it('keeps existing coordinates stable when unrelated nodes are added', () => {
    const base = fixture().nodes
    const before = radialPositions(base)
    const after = radialPositions([...base, node({ id: 'application:unrelated', kind: 'application', ring: 4, label: 'Unrelated', groupId: 'group:application:mcp' })])
    for (const item of base) expect(after.get(item.id)).toEqual(before.get(item.id))
  })

  it('lays out a representative aggregated fixture within the interaction budget', () => {
    const large = fixture().nodes.slice(0, 10)
    for (let index = 0; index < 561; index += 1) large.push(node({ id: `skill:${index}`, kind: 'skill', ring: 1, label: `Skill ${index}`, groupId: 'group:skill:test' }))
    for (let index = 0; index < 100; index += 1) large.push(node({ id: `application:${index}`, kind: 'application', ring: 4, label: `Application ${index}`, groupId: 'group:application:mcp' }))
    const started = performance.now()
    const positions = radialPositions(large)
    const elapsed = performance.now() - started
    console.info(`ORBIT_LAYOUT_FIXTURE={"nodes":${large.length},"layoutMs":${elapsed.toFixed(3)}}`)
    expect(positions.size).toBe(large.length)
    expect(elapsed).toBeLessThan(100)
  })
})
