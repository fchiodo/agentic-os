import '@testing-library/jest-dom/vitest'
import { act, cleanup, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { MemoryRouter } from 'react-router-dom'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { orbitMapSchema } from '@/features/memory/schema'
import type { OrbitMap, OrbitNode } from '@/features/memory/schema'

const globeMock = vi.hoisted(() => ({
  focusNode: vi.fn(),
  props: null as import('@/features/memory/orbit-globe').OrbitGlobeProps | null,
  resetView: vi.fn(),
}))

const orbitHookState = vi.hoisted(() => ({
  current: null as OrbitMap | null,
  refetch: vi.fn(),
}))

vi.mock('@/features/memory/orbit-globe', async () => {
  const React = await import('react')
  const OrbitGlobe = React.forwardRef<
    import('@/features/memory/orbit-globe').OrbitGlobeHandle,
    import('@/features/memory/orbit-globe').OrbitGlobeProps
  >((props, ref) => {
    globeMock.props = props
    React.useImperativeHandle(ref, () => ({ focusNode: globeMock.focusNode, resetView: globeMock.resetView }))
    return <div aria-label="Interactive 3D brain graph" />
  })
  return { OrbitGlobe }
})

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
  globeMock.focusNode.mockReset()
  globeMock.resetView.mockReset()
  globeMock.props = null
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
    act(() => globeMock.props?.onNodeClick('group:skill:test'))
    expect(await screen.findByRole('heading', { name: 'Test skills' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Expand group' })).toBeInTheDocument()
    expect(globeMock.focusNode).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Center selection' }))
    expect(globeMock.focusNode).toHaveBeenCalledWith('group:skill:test')
  })

  it('renders the 3D brain categories and scene controls', () => {
    renderMap()
    expect(screen.getByRole('heading', { name: '3D Brain' })).toBeInTheDocument()
    expect(screen.getByLabelText('Interactive 3D brain graph')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Play growth' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Cinema' })).toBeInTheDocument()
    expect(globeMock.props?.nodes.map((item) => item.id)).toContain('core:agentic-os')
  })

  it('starts the growth replay and exposes a clean cinema view', () => {
    const view = renderMap()
    expect(globeMock.props?.replayNonce).toBe(0)

    fireEvent.click(screen.getByRole('button', { name: 'Play growth' }))
    expect(globeMock.props?.replayNonce).toBe(1)

    fireEvent.click(screen.getByRole('button', { name: 'Cinema' }))
    expect(view.container.querySelector('.orbit-workspace')).toHaveClass('is-cinema')
    expect(screen.getByRole('button', { name: 'Exit cinema' })).toBeInTheDocument()
  })

  it('finds an item in a closed group, expands its parent, and opens the detail', async () => {
    renderMap()
    fireEvent.change(screen.getByRole('searchbox', { name: 'Search all map items' }), { target: { value: 'Deep skill' } })
    expect(screen.getByText((_content, element) => element?.textContent === '1 result in authorized data')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('option', { name: /Deep skill/ }))

    expect(await screen.findByRole('heading', { name: 'Deep skill' })).toBeInTheDocument()
    expect(globeMock.focusNode).toHaveBeenCalledWith('skill:deep')
    fireEvent.click(screen.getByText('Explore as an accessible list'))
    expect(screen.getByRole('button', { name: /Deep skill/ })).toBeInTheDocument()
  })

  it('does not move the camera on a payload refresh and closes selection with Escape', async () => {
    const view = renderMap()
    act(() => globeMock.props?.onNodeClick('group:skill:test'))
    fireEvent.click(await screen.findByRole('button', { name: 'Center selection' }))
    globeMock.focusNode.mockClear()

    orbitHookState.current = fixture('2026-09-23T10:01:00Z')
    view.rerender(<MemoryRouter><OrbitMapView onOpenMemory={vi.fn()} /></MemoryRouter>)
    await act(async () => {})
    expect(globeMock.focusNode).not.toHaveBeenCalled()

    fireEvent.keyDown(window, { key: 'Escape' })
    await waitFor(() => expect(screen.queryByLabelText('Map selection details')).not.toBeInTheDocument())
  })

  it('keeps a large group readable and never reveals every structural child on selection', async () => {
    const current = fixture()
    const bulkNodes = Array.from({ length: 60 }, (_, index) => node({
      id: `skill:bulk-${index.toString().padStart(2, '0')}`,
      kind: 'skill',
      ring: 1,
      label: `Bulk skill ${index + 1}`,
      groupId: 'group:skill:test',
    }))
    const bulkEdges = bulkNodes.map((child, index) => ({
      id: `edge:bulk-${index}`,
      source: 'group:skill:test',
      target: child.id,
      relation: 'contains',
      evidence: 'declared' as const,
      weight: 1,
      activityAt: null,
      provenance: [{ kind: 'registry', reference: child.sourceRef, detail: 'Discovered skill', ts: null }],
    }))
    orbitHookState.current = { ...current, nodes: [...current.nodes, ...bulkNodes], edges: [...current.edges, ...bulkEdges] }
    renderMap()

    act(() => globeMock.props?.onNodeClick('group:skill:test'))
    expect(globeMock.props?.nodes.filter((item) => item.id.startsWith('skill:bulk-'))).toHaveLength(0)
    expect(globeMock.props?.renderedEdgeIds.has('edge:bulk-0')).toBe(false)

    fireEvent.click(await screen.findByRole('button', { name: 'Expand group' }))
    await waitFor(() => expect(globeMock.props?.nodes.filter((item) => item.id.startsWith('skill:bulk-'))).toHaveLength(12))
    expect(screen.getByText((_content, element) => element?.textContent === '12 of 61 items shown')).toBeInTheDocument()
  })

  it('finds a late child without expanding hundreds of siblings', async () => {
    const current = fixture()
    const bulkNodes = Array.from({ length: 60 }, (_, index) => node({
      id: `skill:bulk-${index.toString().padStart(2, '0')}`,
      kind: 'skill',
      ring: 1,
      label: `Bulk skill ${index + 1}`,
      groupId: 'group:skill:test',
    }))
    orbitHookState.current = { ...current, nodes: [...current.nodes, ...bulkNodes] }
    renderMap()

    fireEvent.change(screen.getByRole('searchbox', { name: 'Search all map items' }), { target: { value: 'Bulk skill 60' } })
    fireEvent.click(screen.getByRole('option', { name: /Bulk skill 60/ }))

    expect(await screen.findByRole('heading', { name: 'Bulk skill 60' })).toBeInTheDocument()
    expect(globeMock.props?.nodes.some((item) => item.id === 'skill:bulk-59')).toBe(true)
    expect(globeMock.props?.nodes.filter((item) => item.id.startsWith('skill:bulk-')).length).toBeLessThanOrEqual(13)
  })

  it('separates recorded activity from unavailable telemetry', async () => {
    renderMap()
    fireEvent.click(screen.getByRole('button', { name: 'Activity' }))
    fireEvent.click(screen.getByRole('button', { name: /Run governed task/ }))
    expect(await screen.findByRole('heading', { name: 'Run governed task' })).toBeInTheDocument()
    expect(screen.getByText('2 recorded events')).toBeInTheDocument()
    expect(screen.getByText(/not claimed to have determined the answer/i)).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Trace' })).not.toBeInTheDocument()

    fireEvent.click(screen.getByRole('button', { name: /Task without trace/ }))
    expect(await screen.findByText(/Data not available: this task has no recorded events/i)).toBeInTheDocument()
  })

  it('keeps the selected task while inspecting one of its linked nodes', async () => {
    renderMap()
    fireEvent.click(screen.getByRole('button', { name: 'Activity' }))
    const taskButton = screen.getByRole('button', { name: /Run governed task/ })
    fireEvent.click(taskButton)
    act(() => globeMock.props?.onNodeClick('skill:deep'))

    expect(await screen.findByRole('heading', { name: 'Deep skill' })).toBeInTheDocument()
    expect(screen.getByText('Inspecting within task: Run governed task')).toBeInTheDocument()
    expect(taskButton).toHaveAttribute('aria-pressed', 'true')
    expect(globeMock.props?.renderedEdgeIds.has('activity:task-1:0')).toBe(true)
  })

  it('applies evidence filters to selected-node and activity relations', async () => {
    renderMap()
    act(() => globeMock.props?.onNodeClick('skill:deep'))
    expect(globeMock.props?.renderedEdgeIds.has('edge:observed')).toBe(true)

    fireEvent.click(screen.getByRole('button', { name: 'Show relations' }))
    fireEvent.click(screen.getByRole('checkbox', { name: 'Observed' }))
    await waitFor(() => expect(globeMock.props?.renderedEdgeIds.has('edge:observed')).toBe(false))

    fireEvent.click(screen.getByRole('button', { name: 'Activity' }))
    fireEvent.click(screen.getByRole('button', { name: /Run governed task/ }))
    expect(globeMock.props?.renderedEdgeIds.has('activity:task-1:0')).toBe(false)
  })

  it('routes registry evidence to Catalog instead of presenting it as a trace', async () => {
    renderMap()
    act(() => globeMock.props?.onNodeClick('skill:deep'))
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
