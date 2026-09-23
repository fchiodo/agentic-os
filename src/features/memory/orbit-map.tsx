import Graph from 'graphology'
import {
  CheckCircle2,
  ChevronRight,
  ExternalLink,
  Layers3,
  Network,
  RefreshCw,
  Search,
  ShieldCheck,
} from 'lucide-react'
import Sigma from 'sigma'
import { useEffect, useMemo, useRef, useState } from 'react'
import { StatusBadge } from '@/components/ui/status-badge'
import { useMemoryConfirm, useMemoryOrbitMap } from '@/features/memory/hooks'
import type { OrbitEdge, OrbitNode } from '@/features/memory/schema'
import { formatCompactNumber } from '@/lib/format'
import { useTaskEventsStore } from '@/store/task-events'

const DOMAINS = ['work', 'planphysique', 'personal', 'family', 'finance', 'research'] as const

const DOMAIN_LABELS: Record<string, string> = {
  work: 'Work',
  planphysique: 'PlanPhysique',
  personal: 'Personal',
  family: 'Family',
  finance: 'Finance',
  research: 'Research',
}

const RING_META = [
  { color: '#141413', label: 'AgenticOS', radius: 0 },
  { color: '#c6613f', label: 'Skills', radius: 11 },
  { color: '#6a9bcc', label: 'Memory', radius: 21 },
  { color: '#788c5d', label: 'Routines', radius: 31 },
  { color: '#8b6f9c', label: 'Applications', radius: 41 },
] as const

const RING_PHASE = [0, -0.55, 0.18, 0.92, 1.55] as const

const EVIDENCE_COLOR: Record<OrbitEdge['evidence'], string> = {
  declared: '#9b978e',
  observed: '#c6613f',
  inferred: '#6a9bcc',
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function stableHash(value: string): number {
  let hash = 2166136261
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  return hash >>> 0
}

function orderedTopLevel(nodes: OrbitNode[], ring: number): OrbitNode[] {
  return nodes
    .filter((node) => node.ring === ring && node.groupId === null)
    .sort((left, right) => left.id.localeCompare(right.id))
}

function radialPositions(nodes: OrbitNode[]): Map<string, { x: number; y: number }> {
  const positions = new Map<string, { x: number; y: number }>()
  positions.set('core:agentic-os', { x: 0, y: 0 })
  const groupAngles = new Map<string, number>()

  for (let ring = 1; ring <= 4; ring += 1) {
    const topLevel = orderedTopLevel(nodes, ring)
    topLevel.forEach((node, index) => {
      const stableJitter = topLevel.length > 1 ? (stableHash(`ring:${ring}`) % 100) / 1200 : 0
      const angle = -Math.PI / 2 + RING_PHASE[ring] + stableJitter + (Math.PI * 2 * index) / Math.max(1, topLevel.length)
      const radius = RING_META[ring].radius
      positions.set(node.id, { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius })
      groupAngles.set(node.id, angle)
    })
  }

  const childrenByGroup = new Map<string, OrbitNode[]>()
  for (const node of nodes) {
    if (!node.groupId) continue
    const children = childrenByGroup.get(node.groupId) ?? []
    children.push(node)
    childrenByGroup.set(node.groupId, children)
  }

  for (const [groupId, children] of childrenByGroup) {
    const baseAngle = groupAngles.get(groupId) ?? 0
    const ordered = [...children].sort((left, right) => left.id.localeCompare(right.id))
    const spread = Math.min(0.72, Math.max(0.22, ordered.length * 0.035))
    ordered.forEach((node, index) => {
      const fraction = ordered.length === 1 ? 0 : index / (ordered.length - 1) - 0.5
      const angle = baseAngle + fraction * spread
      const lane = index % 3
      const radius = RING_META[node.ring].radius + 2.4 + lane * 1.15
      positions.set(node.id, { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius })
    })
  }
  return positions
}

function nodeSize(node: OrbitNode): number {
  if (node.kind === 'core') return 17
  if (node.aggregate) return Math.min(13, 7 + Math.log2(node.count + 1) * 1.5)
  return node.kind === 'memory' ? 4.5 : 5.5
}

function makeGraph(
  nodes: OrbitNode[],
  edges: OrbitEdge[],
  visibleIds: Set<string>,
): Graph {
  const graph = new Graph({ multi: true, type: 'directed' })
  const positions = radialPositions(nodes)
  // Invisible cardinal anchors keep the camera centred on AgenticOS even
  // when one ring contains only one or two highly asymmetric groups.
  const anchors: Array<[string, number, number]> = [
    ['__layout:north', 0, -45],
    ['__layout:east', 45, 0],
    ['__layout:south', 0, 45],
    ['__layout:west', -45, 0],
  ]
  anchors.forEach(([id, x, y]) => graph.addNode(id, {
    x,
    y,
    size: 0.01,
    label: '',
    color: '#ffffff00',
    hidden: true,
  }))
  for (const node of nodes) {
    if (!visibleIds.has(node.id)) continue
    const position = positions.get(node.id) ?? { x: 0, y: 0 }
    graph.addNode(node.id, {
      ...position,
      label: node.label,
      size: nodeSize(node),
      color: RING_META[node.ring].color,
      forceLabel: node.aggregate || node.kind === 'core',
      nodeKind: node.kind,
      status: node.status,
      zIndex: node.aggregate ? 2 : 1,
    })
  }
  for (const edge of edges) {
    if (!visibleIds.has(edge.source) || !visibleIds.has(edge.target)) continue
    if (!graph.hasNode(edge.source) || !graph.hasNode(edge.target)) continue
    graph.addEdgeWithKey(edge.id, edge.source, edge.target, {
      color: EVIDENCE_COLOR[edge.evidence],
      size: Math.min(3, 0.55 + Math.log2(edge.weight + 1) * 0.45),
      evidence: edge.evidence,
      relation: edge.relation,
      recentActivity: edge.activityAt !== null
        && Date.now() - Date.parse(edge.activityAt) >= 0
        && Date.now() - Date.parse(edge.activityAt) < 30_000,
      zIndex: edge.evidence === 'observed' ? 2 : 1,
    })
  }
  return graph
}

function rollupEdges(
  nodes: OrbitNode[],
  edges: OrbitEdge[],
  visibleIds: Set<string>,
): OrbitEdge[] {
  const nodeById = new Map(nodes.map((node) => [node.id, node]))
  const endpoint = (id: string) => {
    if (visibleIds.has(id)) return id
    const groupId = nodeById.get(id)?.groupId
    return groupId && visibleIds.has(groupId) ? groupId : null
  }
  const rolled = new Map<string, OrbitEdge>()
  for (const edge of edges) {
    const source = endpoint(edge.source)
    const target = endpoint(edge.target)
    if (!source || !target || source === target) continue
    const key = `${source}|${target}|${edge.relation}|${edge.evidence}`
    const existing = rolled.get(key)
    if (existing) {
      existing.weight += edge.weight
      existing.provenance = [...existing.provenance, ...edge.provenance].slice(0, 8)
      if ((edge.activityAt ?? '') > (existing.activityAt ?? '')) {
        existing.activityAt = edge.activityAt
      }
      continue
    }
    rolled.set(key, {
      ...edge,
      id: source === edge.source && target === edge.target
        ? edge.id
        : `rollup:${stableHash(key).toString(16)}`,
      source,
      target,
      provenance: edge.provenance.slice(0, 8),
    })
  }
  return [...rolled.values()]
}

function relationLabel(edge: OrbitEdge, selectedId: string): string {
  return edge.source === selectedId ? edge.relation : `is ${edge.relation} by`
}

export function OrbitMapView({ onOpenMemory }: { onOpenMemory: (path: string) => void }) {
  const canvasRef = useRef<HTMLDivElement | null>(null)
  const rendererRef = useRef<Sigma | null>(null)
  const [domain, setDomain] = useState<string | undefined>()
  const [includeSensitive, setIncludeSensitive] = useState(false)
  const [search, setSearch] = useState('')
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(() => new Set())
  const [selectedId, setSelectedId] = useState<string>('core:agentic-os')
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null)
  const [livePulse, setLivePulse] = useState(false)
  const lastTaskEvent = useTaskEventsStore((state) => state.lastEvent)
  const orbitQuery = useMemoryOrbitMap(domain, includeSensitive)
  const refetchOrbit = orbitQuery.refetch
  const confirmMutation = useMemoryConfirm()

  const nodeById = useMemo(
    () => new Map((orbitQuery.data?.nodes ?? []).map((node) => [node.id, node])),
    [orbitQuery.data?.nodes],
  )
  const selectedNode = nodeById.get(selectedId) ?? null
  const query = search.trim().toLocaleLowerCase()

  const visibleIds = useMemo(() => {
    const visible = new Set<string>()
    const nodes = orbitQuery.data?.nodes ?? []
    for (const node of nodes) {
      const matchesSearch = query.length === 0
        || [node.label, node.subtitle ?? '', node.domain ?? '', node.status, node.sourceRef]
          .join(' ')
          .toLocaleLowerCase()
          .includes(query)
      if (node.kind === 'core' || node.groupId === null || expandedGroups.has(node.groupId) || (query && matchesSearch)) {
        visible.add(node.id)
        if (node.groupId) visible.add(node.groupId)
      }
    }
    visible.add('core:agentic-os')
    return visible
  }, [expandedGroups, orbitQuery.data?.nodes, query])

  const visibleEdges = useMemo(
    () => rollupEdges(orbitQuery.data?.nodes ?? [], orbitQuery.data?.edges ?? [], visibleIds),
    [orbitQuery.data?.edges, orbitQuery.data?.nodes, visibleIds],
  )
  const selectedEdge = visibleEdges.find((edge) => edge.id === selectedEdgeId) ?? null

  const graph = useMemo(
    () => makeGraph(orbitQuery.data?.nodes ?? [], visibleEdges, visibleIds),
    [orbitQuery.data?.nodes, visibleEdges, visibleIds],
  )

  useEffect(() => {
    if (!lastTaskEvent || Date.now() - Date.parse(lastTaskEvent.ts) > 10_000) return
    const pulseStartTimer = window.setTimeout(() => setLivePulse(true), 0)
    const refreshTimer = window.setTimeout(() => void refetchOrbit(), 250)
    const pulseTimer = window.setTimeout(() => setLivePulse(false), 1_600)
    return () => {
      window.clearTimeout(pulseStartTimer)
      window.clearTimeout(refreshTimer)
      window.clearTimeout(pulseTimer)
    }
  }, [lastTaskEvent, refetchOrbit])

  useEffect(() => {
    const container = canvasRef.current
    if (!container || graph.order === 0) return
    const renderer = new Sigma(graph, container, {
      allowInvalidContainer: true,
      defaultEdgeColor: '#b0aea5',
      defaultNodeColor: '#87867f',
      enableEdgeEvents: true,
      labelColor: { color: '#3d3d3a' },
      labelDensity: 0.8,
      labelFont: 'Anthropic Sans, Arial, sans-serif',
      labelRenderedSizeThreshold: 5,
      labelSize: 11,
      renderEdgeLabels: false,
      zIndex: true,
    })
    rendererRef.current = renderer
    renderer.on('clickNode', ({ node }) => {
      const selected = nodeById.get(node)
      setSelectedId(node)
      setSelectedEdgeId(null)
      if (selected?.aggregate) {
        setExpandedGroups((current) => {
          const next = new Set(current)
          if (next.has(node)) next.delete(node)
          else next.add(node)
          return next
        })
      }
    })
    renderer.on('clickEdge', ({ edge }) => {
      setSelectedEdgeId(edge)
    })
    renderer.on('clickStage', () => {
      setSelectedEdgeId(null)
    })
    return () => {
      renderer.kill()
      rendererRef.current = null
    }
  }, [graph, nodeById])

  useEffect(() => {
    const renderer = rendererRef.current
    if (!renderer || !graph.hasNode(selectedId)) return
    const neighbors = new Set(graph.neighbors(selectedId))
    renderer.setSetting('nodeReducer', (node, data) => {
      if (node === selectedId) return { ...data, highlighted: true, forceLabel: true, size: data.size * 1.2 }
      if (selectedId && !neighbors.has(node)) return { ...data, color: '#d1cfc5', label: '' }
      return data
    })
    renderer.setSetting('edgeReducer', (edge, data) => {
      if (graph.source(edge) === selectedId || graph.target(edge) === selectedId) {
        return { ...data, color: data.recentActivity && livePulse ? '#c6613f' : '#141413', size: Math.max(data.recentActivity && livePulse ? 3.4 : 1.4, data.size), zIndex: 4 }
      }
      return { ...data, color: '#e0ded6', hidden: Boolean(selectedId) }
    })
    renderer.refresh()
  }, [graph, livePulse, selectedId])

  const relations = useMemo(() => {
    if (!orbitQuery.data || !selectedNode) return []
    return visibleEdges
      .filter((edge) => edge.source === selectedNode.id || edge.target === selectedNode.id)
      .sort((left, right) => right.weight - left.weight)
  }, [orbitQuery.data, selectedNode, visibleEdges])

  const toggleSelectedGroup = () => {
    if (!selectedNode?.aggregate) return
    setExpandedGroups((current) => {
      const next = new Set(current)
      if (next.has(selectedNode.id)) next.delete(selectedNode.id)
      else next.add(selectedNode.id)
      return next
    })
  }

  return (
    <div className="orbit-workspace">
      <section className="surface orbit-toolbar">
        <div>
          <p className="eyebrow">Operational graph</p>
          <h2>Orbital map</h2>
        </div>
        <label className="orbit-search">
          <Search aria-hidden="true" size={16} />
          <input onChange={(event) => setSearch(event.target.value)} placeholder="Search nodes and sources" type="search" value={search} />
        </label>
        <select aria-label="Filter map by domain" onChange={(event) => setDomain(event.target.value || undefined)} value={domain ?? ''}>
          <option value="">All domains</option>
          {DOMAINS.map((item) => <option key={item} value={item}>{DOMAIN_LABELS[item]}</option>)}
        </select>
        <label className="memory-toggle-label orbit-sensitive-toggle">
          <input checked={includeSensitive} onChange={(event) => setIncludeSensitive(event.target.checked)} type="checkbox" />
          Sensitive
        </label>
        <button className="secondary-button" disabled={orbitQuery.isFetching} onClick={() => void orbitQuery.refetch()} type="button">
          <RefreshCw aria-hidden="true" className={orbitQuery.isFetching ? 'is-spinning' : ''} size={15} />
          Refresh
        </button>
        {livePulse && <span className="orbit-live-activity" role="status">Live activity</span>}
      </section>

      {orbitQuery.error && <div className="inline-error" role="alert">{errorMessage(orbitQuery.error)}</div>}

      <div className="orbit-main">
        <section className="surface orbit-stage-shell">
          <div className="orbit-ring-legend" aria-label="Map rings">
            {RING_META.slice(1).map((ring, index) => (
              <span key={ring.label}><i style={{ background: ring.color }} />{ring.label}<strong>{formatCompactNumber([orbitQuery.data?.counts.skills ?? 0, orbitQuery.data?.counts.memories ?? 0, orbitQuery.data?.counts.routines ?? 0, orbitQuery.data?.counts.applications ?? 0][index])}</strong></span>
            ))}
          </div>
          {orbitQuery.isLoading ? (
            <div className="orbit-loading"><Network aria-hidden="true" size={34} /><p>Composing local registries…</p></div>
          ) : (
            <div className="orbit-canvas" ref={canvasRef} />
          )}
          <div className="orbit-evidence-legend">
            {(['declared', 'observed', 'inferred'] as const).map((evidence) => <span key={evidence}><i style={{ background: EVIDENCE_COLOR[evidence] }} />{evidence}</span>)}
            {orbitQuery.data && <span>{orbitQuery.data.metrics.composeMs.toFixed(1)} ms · {orbitQuery.data.metrics.tracesScanned} traces</span>}
          </div>
        </section>

        <aside className="surface orbit-detail">
          {selectedEdge ? (
            <>
              <div className="orbit-detail-heading"><Network aria-hidden="true" size={18} /><div><p className="eyebrow">Relation</p><h2>{selectedEdge.relation}</h2></div></div>
              <div className="orbit-detail-badges"><StatusBadge label={selectedEdge.evidence} tone={selectedEdge.evidence === 'observed' ? 'success' : selectedEdge.evidence === 'inferred' ? 'warning' : 'neutral'} /><span>{selectedEdge.weight} event{selectedEdge.weight === 1 ? '' : 's'}</span></div>
              <dl className="orbit-detail-meta"><dt>From</dt><dd>{nodeById.get(selectedEdge.source)?.label ?? selectedEdge.source}</dd><dt>To</dt><dd>{nodeById.get(selectedEdge.target)?.label ?? selectedEdge.target}</dd></dl>
              <div className="orbit-provenance"><h3>Provenance</h3>{selectedEdge.provenance.map((item, index) => <div key={`${item.reference}-${index}`}><strong>{item.kind}</strong><span>{item.detail}</span><code>{item.reference}</code></div>)}</div>
              <button className="secondary-button" onClick={() => setSelectedEdgeId(null)} type="button">Back to node</button>
            </>
          ) : selectedNode ? (
            <>
              <div className="orbit-detail-heading"><Layers3 aria-hidden="true" size={18} /><div><p className="eyebrow">{RING_META[selectedNode.ring].label}</p><h2>{selectedNode.label}</h2></div></div>
              <div className="orbit-detail-badges"><StatusBadge label={selectedNode.status} tone={selectedNode.status === 'stale' ? 'warning' : 'neutral'} />{selectedNode.domain && <span>{DOMAIN_LABELS[selectedNode.domain] ?? selectedNode.domain}</span>}{selectedNode.sensitivity && <span><ShieldCheck aria-hidden="true" size={13} />{selectedNode.sensitivity}</span>}</div>
              {selectedNode.preview && <p className="orbit-preview">{selectedNode.preview}</p>}
              <dl className="orbit-detail-meta"><dt>Source</dt><dd><code>{selectedNode.sourceRef}</code></dd>{selectedNode.sourcePath && <><dt>Path</dt><dd><code>{selectedNode.sourcePath}</code></dd></>}<dt>Visible items</dt><dd>{selectedNode.count}</dd></dl>
              <div className="orbit-detail-actions">
                {selectedNode.aggregate && <button className="secondary-button" onClick={toggleSelectedGroup} type="button">{expandedGroups.has(selectedNode.id) ? 'Collapse group' : 'Expand group'}</button>}
                {selectedNode.actions.includes('open_memory') && selectedNode.sourcePath && <button className="primary-button" onClick={() => onOpenMemory(selectedNode.sourcePath!)} type="button"><ExternalLink aria-hidden="true" size={14} />Open in Memory</button>}
                {selectedNode.actions.includes('confirm_memory') && <button className="secondary-button" disabled={confirmMutation.isPending} onClick={() => confirmMutation.mutate(selectedNode.id.replace(/^memory:/, ''))} type="button"><CheckCircle2 aria-hidden="true" size={14} />Confirm still true</button>}
              </div>
              <div className="orbit-relations"><h3>Relations <span>{relations.length}</span></h3>{relations.slice(0, 18).map((edge) => { const otherId = edge.source === selectedNode.id ? edge.target : edge.source; const other = nodeById.get(otherId); return <button key={edge.id} onClick={() => setSelectedEdgeId(edge.id)} type="button"><span><strong>{relationLabel(edge, selectedNode.id)}</strong><small>{edge.evidence} · {edge.weight}</small></span><span>{other?.label ?? otherId}</span><ChevronRight aria-hidden="true" size={14} /></button> })}{relations.length === 0 && <p className="row-subtle">No governed relation is recorded for this node.</p>}</div>
            </>
          ) : <div className="empty-state"><h3>Select a node</h3><p>Inspect its registry source and governed relations.</p></div>}
        </aside>
      </div>
    </div>
  )
}
