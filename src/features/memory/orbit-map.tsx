import {
  Activity,
  AppWindow,
  ArrowLeft,
  CheckCircle2,
  ChevronRight,
  Crosshair,
  Database,
  ExternalLink,
  Eye,
  EyeOff,
  FileText,
  Layers3,
  ListTree,
  Maximize2,
  Minimize2,
  Network,
  Pause,
  Play,
  RefreshCw,
  RotateCcw,
  Search,
  ShieldCheck,
  Wand2,
  Workflow,
  X,
} from 'lucide-react'
import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { StatusBadge } from '@/components/ui/status-badge'
import { useMemoryConfirm, useMemoryOrbitMap } from '@/features/memory/hooks'
import { OrbitGlobe, type OrbitGlobeHandle, type ReplayProgress } from '@/features/memory/orbit-globe'
import type {
  OrbitActivity,
  OrbitActivityWindow,
  OrbitEdge,
  OrbitNode,
} from '@/features/memory/schema'
import { formatCompactNumber, formatRelativeTime } from '@/lib/format'
import { useWorkbenchStore } from '@/store/workbench'

const DOMAINS = ['work', 'planphysique', 'personal', 'family', 'finance', 'research'] as const
const DEFAULT_CHILD_LIMIT = 12
const SEARCH_PAGE_SIZE = 40
const DEFAULT_EDGE_LIMIT = 120
const SELECTED_RELATION_NODE_LIMIT = 18
const STRUCTURAL_RELATIONS = new Set(['contains', 'contains_source', 'governs', 'registers'])

type MapMode = 'structure' | 'activity'
type InteractionKind = 'expansionMs' | 'selectionMs'

const DOMAIN_LABELS: Record<string, string> = {
  work: 'Work',
  planphysique: 'PlanPhysique',
  personal: 'Personal',
  family: 'Family',
  finance: 'Finance',
  research: 'Research',
}

const RING_META = [
  { color: '#f4f1e9', icon: Layers3, label: 'Agentic OS' },
  { color: '#ef835d', icon: Wand2, label: 'Skills' },
  { color: '#6fa8dc', icon: Database, label: 'Memory' },
  { color: '#83a866', icon: Workflow, label: 'Routines' },
  { color: '#a788c0', icon: AppWindow, label: 'Applications' },
] as const

const EMPTY_NODES: OrbitNode[] = []
const EMPTY_EDGES: OrbitEdge[] = []
const DEFAULT_RELATION_TYPES = [
  'contains',
  'contains_source',
  'consulted',
  'derived_from',
  'executed',
  'governs',
  'inserted_into_context',
  'produced',
  'related_to',
  'supersedes',
  'used',
]

const EVIDENCE_META: Record<OrbitEdge['evidence'], { color: string; label: string; style: string }> = {
  declared: { color: '#9e9a90', label: 'Declared', style: 'thin path' },
  observed: { color: '#f08a62', label: 'Observed', style: 'particle path' },
  inferred: { color: '#70a6d8', label: 'Inferred', style: 'faint path' },
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function relationLabel(edge: OrbitEdge, selectedId: string): string {
  return edge.source === selectedId ? edge.relation : `target of ${edge.relation}`
}

function kindLabel(node: OrbitNode): string {
  const labels: Record<string, string> = {
    application: 'Application', application_group: 'Application group', core: 'Control plane',
    memory: 'Memory', memory_domain: 'Memory domain', routine: 'Routine', routine_group: 'Routine group',
    skill: 'Skill', skill_group: 'Skill group', source: 'Original source',
  }
  return labels[node.kind] ?? node.kind.replaceAll('_', ' ')
}

function nodeDescription(node: OrbitNode): string {
  if (node.preview) return node.preview
  if (node.kind === 'memory_domain') return `${node.count} authorized memories in this domain.`
  if (node.aggregate) return `${node.count} registered items. Expand the group to inspect them.`
  if (node.kind === 'source') return 'Original document preserved by the governed import pipeline.'
  return node.subtitle ?? 'Registered in the local Agentic OS inventory.'
}

function catalogId(node: OrbitNode): string | null {
  if (!['skill', 'routine', 'application'].includes(node.kind)) return null
  return node.id.slice(node.id.indexOf(':') + 1)
}

function activityEdges(activity: OrbitActivity | null): OrbitEdge[] {
  if (!activity) return []
  return activity.links.map((link, index) => ({
    id: `activity:${activity.taskId}:${index}`,
    source: 'core:agentic-os',
    target: link.nodeId,
    relation: link.relation,
    evidence: 'observed',
    weight: 1,
    activityAt: link.occurredAt,
    provenance: [{ kind: 'execution_event', reference: link.eventRef, detail: link.detail, ts: link.occurredAt }],
  }))
}

function stateLabel(value: string): string {
  return value.replaceAll('_', ' ')
}

export function OrbitMapView({
  onOpenMemory,
  onOpenSource,
}: {
  onOpenMemory: (path: string) => void
  onOpenSource?: (importId: string, domain?: string) => void
}) {
  const navigate = useNavigate()
  const setCatalogFilter = useWorkbenchStore((state) => state.setCatalogFilter)
  const setCatalogSearch = useWorkbenchStore((state) => state.setCatalogSearch)
  const setSelectedCatalogId = useWorkbenchStore((state) => state.setSelectedCatalogId)
  const globeRef = useRef<OrbitGlobeHandle | null>(null)
  const interactionStartedRef = useRef<Partial<Record<InteractionKind, number>>>({})
  const firstRenderStartedRef = useRef<number | null>(null)
  const firstRenderedPayloadRef = useRef<string | null>(null)
  const searchStartedRef = useRef<number | null>(null)

  const [mode, setMode] = useState<MapMode>('structure')
  const [activityWindow, setActivityWindow] = useState<OrbitActivityWindow>('today')
  const [domain, setDomain] = useState<string | undefined>()
  const [includeSensitive, setIncludeSensitive] = useState(false)
  const [search, setSearch] = useState('')
  const [searchLimit, setSearchLimit] = useState(SEARCH_PAGE_SIZE)
  const [expandedGroups, setExpandedGroups] = useState<Set<string>>(() => new Set())
  const [groupLimits, setGroupLimits] = useState<Record<string, number>>({})
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [selectedTaskId, setSelectedTaskId] = useState<string | null>(null)
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null)
  const [detailOpen, setDetailOpen] = useState(false)
  const [showRelations, setShowRelations] = useState(false)
  const [relationLimit, setRelationLimit] = useState(DEFAULT_EDGE_LIMIT)
  const [relationTypes, setRelationTypes] = useState<Set<string>>(() => new Set(DEFAULT_RELATION_TYPES))
  const [evidenceTypes, setEvidenceTypes] = useState<Set<OrbitEdge['evidence']>>(() => new Set(['declared', 'observed', 'inferred']))
  const [reducedMotion, setReducedMotion] = useState(() => typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches)
  const [motionEnabled, setMotionEnabled] = useState(() => !(typeof window !== 'undefined' && typeof window.matchMedia === 'function' && window.matchMedia('(prefers-reduced-motion: reduce)').matches))
  const [cinema, setCinema] = useState(false)
  const [replayFullBrain, setReplayFullBrain] = useState(false)
  const [replayNonce, setReplayNonce] = useState(0)
  const [replay, setReplay] = useState<ReplayProgress>({ active: false, complete: false, progress: 0, visible: 0, total: 0 })
  const [uiMetrics, setUiMetrics] = useState({ expansionMs: 0, firstRenderMs: 0, graphBuildMs: 0, searchMs: 0, selectionMs: 0 })

  const orbitQuery = useMemoryOrbitMap(domain, includeSensitive, activityWindow)
  const confirmMutation = useMemoryConfirm()
  const nodes = orbitQuery.data?.nodes ?? EMPTY_NODES
  const allEdges = orbitQuery.data?.edges ?? EMPTY_EDGES
  const nodeById = useMemo(() => new Map(nodes.map((node) => [node.id, node])), [nodes])
  const selectedNode = selectedId ? nodeById.get(selectedId) ?? null : null
  const selectedActivity = orbitQuery.data?.activities.find((activity) => activity.taskId === selectedTaskId) ?? null
  const query = search.trim().toLocaleLowerCase()

  useEffect(() => {
    if (typeof window.matchMedia !== 'function') return
    const media = window.matchMedia('(prefers-reduced-motion: reduce)')
    const onChange = (event: MediaQueryListEvent) => {
      setReducedMotion(event.matches)
      setMotionEnabled(!event.matches)
    }
    media.addEventListener('change', onChange)
    return () => media.removeEventListener('change', onChange)
  }, [])

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') return
      if (cinema) {
        setCinema(false)
        return
      }
      setSelectedId(null)
      setSelectedTaskId(null)
      setSelectedEdgeId(null)
      setDetailOpen(false)
    }
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [cinema])

  const childrenByGroup = useMemo(() => {
    const grouped = new Map<string, OrbitNode[]>()
    for (const node of nodes) {
      if (!node.groupId) continue
      const children = grouped.get(node.groupId) ?? []
      children.push(node)
      grouped.set(node.groupId, children)
    }
    for (const children of grouped.values()) children.sort((left, right) => left.id.localeCompare(right.id))
    return grouped
  }, [nodes])

  const matchingSearchResults = useMemo(() => {
    if (!query) return []
    return nodes
      .filter((node) => node.kind !== 'core')
      .filter((node) => [node.label, node.subtitle ?? '', node.domain ?? '', node.status, node.operationalState, node.catalogState, node.usageState, node.connectionState, node.sourceRef].join(' ').toLocaleLowerCase().includes(query))
      .sort((left, right) => Number(right.aggregate) - Number(left.aggregate) || left.label.localeCompare(right.label))
  }, [nodes, query])
  const searchResults = useMemo(() => matchingSearchResults.slice(0, searchLimit), [matchingSearchResults, searchLimit])
  const relationIsEnabled = useCallback((edge: OrbitEdge) => relationTypes.has(edge.relation) && evidenceTypes.has(edge.evidence), [evidenceTypes, relationTypes])

  useEffect(() => {
    const started = searchStartedRef.current
    if (started === null) return
    const frame = window.requestAnimationFrame(() => {
      setUiMetrics((current) => ({ ...current, searchMs: performance.now() - started }))
      searchStartedRef.current = null
    })
    return () => window.cancelAnimationFrame(frame)
  }, [searchResults])

  const selectedActivityNodeIds = useMemo(() => new Set(selectedActivity?.links.map((link) => link.nodeId) ?? []), [selectedActivity])

  const visibleIds = useMemo(() => {
    const visible = new Set<string>(['core:agentic-os'])
    if (replayFullBrain) for (const node of nodes) visible.add(node.id)
    for (const node of nodes) if (node.groupId === null) visible.add(node.id)
    for (const groupId of expandedGroups) {
      const limit = groupLimits[groupId] ?? DEFAULT_CHILD_LIMIT
      for (const child of (childrenByGroup.get(groupId) ?? []).slice(0, limit)) visible.add(child.id)
    }
    if (selectedNode) {
      visible.add(selectedNode.id)
      if (selectedNode.groupId) visible.add(selectedNode.groupId)
      let relatedCount = 0
      for (const edge of allEdges) {
        if (!relationIsEnabled(edge) || STRUCTURAL_RELATIONS.has(edge.relation)) continue
        const relatedId = edge.source === selectedNode.id ? edge.target : edge.target === selectedNode.id ? edge.source : null
        if (!relatedId) continue
        visible.add(relatedId)
        const parent = nodeById.get(relatedId)?.groupId
        if (parent) visible.add(parent)
        relatedCount += 1
        if (relatedCount >= SELECTED_RELATION_NODE_LIMIT) break
      }
    }
    for (const nodeId of selectedActivityNodeIds) {
      visible.add(nodeId)
      const parent = nodeById.get(nodeId)?.groupId
      if (parent) visible.add(parent)
    }
    return visible
  }, [allEdges, childrenByGroup, expandedGroups, groupLimits, nodeById, nodes, relationIsEnabled, replayFullBrain, selectedActivityNodeIds, selectedNode])

  const taskEdges = useMemo(() => activityEdges(selectedActivity), [selectedActivity])
  const allRelationTypes = useMemo(() => [...new Set([...allEdges, ...taskEdges].map((edge) => edge.relation))].sort(), [allEdges, taskEdges])
  const displayedEdges = useMemo(() => {
    if (mode === 'activity') return taskEdges.filter(relationIsEnabled).slice(0, relationLimit)
    const selected = selectedId ? allEdges.filter((edge) => (edge.source === selectedId || edge.target === selectedId) && relationIsEnabled(edge) && !STRUCTURAL_RELATIONS.has(edge.relation)) : []
    const structural = allEdges.filter((edge) => STRUCTURAL_RELATIONS.has(edge.relation))
    const advanced = showRelations ? allEdges.filter(relationIsEnabled) : []
    const byId = new Map<string, OrbitEdge>()
    for (const edge of [...structural, ...selected, ...advanced]) byId.set(edge.id, edge)
    return [...byId.values()]
      .sort((left, right) => Number(right.source === selectedId || right.target === selectedId) - Number(left.source === selectedId || left.target === selectedId) || (right.activityAt ?? '').localeCompare(left.activityAt ?? ''))
      .slice(0, relationLimit)
  }, [allEdges, mode, relationIsEnabled, relationLimit, selectedId, showRelations, taskEdges])

  const selectedEdge = [...allEdges, ...taskEdges].find((edge) => edge.id === selectedEdgeId) ?? null
  const renderedEdges = useMemo(
    () => displayedEdges.filter((edge) => visibleIds.has(edge.source) && visibleIds.has(edge.target)),
    [displayedEdges, visibleIds],
  )
  const visibleNodes = useMemo(() => nodes.filter((node) => visibleIds.has(node.id)), [nodes, visibleIds])
  const globeEdges = useMemo(() => {
    const byId = new Map<string, OrbitEdge>()
    for (const edge of [...allEdges, ...taskEdges]) {
      if (visibleIds.has(edge.source) && visibleIds.has(edge.target)) byId.set(edge.id, edge)
    }
    return [...byId.values()].slice(0, 1_800)
  }, [allEdges, taskEdges, visibleIds])
  const renderedEdgeIds = useMemo(() => new Set(renderedEdges.map((edge) => edge.id)), [renderedEdges])
  const highlightedIds = useMemo(() => {
    const highlighted = new Set(selectedActivityNodeIds)
    if (selectedId) {
      highlighted.add(selectedId)
      for (const edge of globeEdges) {
        if (edge.source === selectedId) highlighted.add(edge.target)
        if (edge.target === selectedId) highlighted.add(edge.source)
      }
    }
    if (selectedNode?.aggregate && expandedGroups.has(selectedNode.id)) {
      const limit = groupLimits[selectedNode.id] ?? DEFAULT_CHILD_LIMIT
      for (const child of (childrenByGroup.get(selectedNode.id) ?? []).slice(0, limit)) highlighted.add(child.id)
    }
    return highlighted
  }, [childrenByGroup, expandedGroups, globeEdges, groupLimits, selectedActivityNodeIds, selectedId, selectedNode])

  useEffect(() => {
    const generatedAt = orbitQuery.data?.generatedAt
    if (!generatedAt || firstRenderedPayloadRef.current === generatedAt) return
    firstRenderStartedRef.current = performance.now()
    const frame = window.requestAnimationFrame(() => {
      window.requestAnimationFrame(() => {
        const started = firstRenderStartedRef.current
        if (started !== null) setUiMetrics((current) => ({ ...current, firstRenderMs: performance.now() - started }))
        firstRenderedPayloadRef.current = generatedAt
      })
    })
    return () => window.cancelAnimationFrame(frame)
  }, [orbitQuery.data?.generatedAt])

  const requestFocus = useCallback((id: string) => {
    globeRef.current?.focusNode(id)
  }, [])

  const recordGraphPerformance = useCallback((graphBuildMs: number) => {
    const now = performance.now()
    const nextMetrics: Partial<typeof uiMetrics> = { graphBuildMs }
    for (const key of ['expansionMs', 'selectionMs'] as InteractionKind[]) {
      const started = interactionStartedRef.current[key]
      if (started !== undefined) {
        nextMetrics[key] = now - started
        delete interactionStartedRef.current[key]
      }
    }
    setUiMetrics((current) => ({ ...current, ...nextMetrics }))
  }, [])

  const playGrowth = () => {
    setSelectedId(null)
    setSelectedTaskId(null)
    setSelectedEdgeId(null)
    setDetailOpen(false)
    setReplayFullBrain(true)
    setReplayNonce((current) => current + 1)
  }

  const selectNode = useCallback((id: string, focus = false) => {
    const node = nodeById.get(id)
    if (!node) return
    interactionStartedRef.current.selectionMs = performance.now()
    if (node.groupId) setExpandedGroups((current) => new Set(current).add(node.groupId!))
    setSelectedId(id)
    setSelectedEdgeId(null)
    setDetailOpen(true)
    if (focus) window.requestAnimationFrame(() => requestFocus(id))
  }, [nodeById, requestFocus])

  const toggleGroup = useCallback((groupId: string) => {
    interactionStartedRef.current.expansionMs = performance.now()
    setExpandedGroups((current) => {
      const next = new Set(current)
      if (next.has(groupId)) next.delete(groupId)
      else next.add(groupId)
      return next
    })
  }, [])

  const selectTask = useCallback((activity: OrbitActivity) => {
    interactionStartedRef.current.selectionMs = performance.now()
    setMode('activity')
    setSelectedTaskId(activity.taskId)
    setSelectedId(null)
    setSelectedEdgeId(null)
    setDetailOpen(true)
    setExpandedGroups((current) => {
      const next = new Set(current)
      for (const link of activity.links) {
        const parent = nodeById.get(link.nodeId)?.groupId
        if (parent) next.add(parent)
      }
      return next
    })
  }, [nodeById])

  const changeSearch = useCallback((value: string) => {
    searchStartedRef.current = performance.now()
    setSearchLimit(SEARCH_PAGE_SIZE)
    setSearch(value)
  }, [])

  const resetOverview = () => {
    setExpandedGroups(new Set())
    setGroupLimits({})
    setSelectedId(null)
    setSelectedTaskId(null)
    setSelectedEdgeId(null)
    setShowRelations(false)
    setSearch('')
    setSearchLimit(SEARCH_PAGE_SIZE)
    setDetailOpen(false)
    setReplayFullBrain(false)
    setReplay({ active: false, complete: false, progress: 0, visible: 0, total: 0 })
    globeRef.current?.resetView()
  }

  const goBackLevel = () => {
    if (selectedEdgeId) return setSelectedEdgeId(null)
    if (selectedTaskId && selectedId) return setSelectedId(null)
    if (selectedNode?.groupId) return selectNode(selectedNode.groupId)
    if (selectedTaskId || selectedId) {
      setSelectedTaskId(null)
      setSelectedId(null)
    }
  }

  const navigateCatalog = (node: OrbitNode) => {
    const id = catalogId(node)
    if (!id) return
    setSelectedCatalogId(id)
    setCatalogSearch(node.label)
    setCatalogFilter(node.kind === 'skill' || node.kind === 'routine' ? node.kind : 'all')
    void navigate('/catalog')
  }

  const openCatalogReference = (reference: string) => {
    const node = nodes.find((candidate) => candidate.sourceRef === reference && catalogId(candidate))
    if (node) return navigateCatalog(node)
    const [, kind = 'all', ...labelParts] = reference.split(':')
    setSelectedCatalogId(null)
    setCatalogSearch(labelParts.join(':'))
    setCatalogFilter(kind === 'skill' || kind === 'routine' ? kind : 'all')
    void navigate('/catalog')
  }

  const evidenceAction = (reference: string): { label: string; run: () => void } | null => {
    if (reference.startsWith('audit:')) return null
    if (reference.startsWith('catalog:')) return { label: 'Open in Catalog', run: () => openCatalogReference(reference) }
    if (reference.startsWith('document-import:') && onOpenSource) {
      const importId = reference.replace(/^document-import:/, '')
      const source = nodes.find((node) => node.id === `source:${importId}`)
      return { label: 'Open original source', run: () => onOpenSource(importId, source?.domain ?? undefined) }
    }
    const sourceNode = nodes.find((node) => node.sourcePath === reference || node.sourceRef === reference)
    if (sourceNode?.kind === 'source' && onOpenSource) {
      return { label: 'Open original source', run: () => onOpenSource(sourceNode.id.replace(/^source:/, ''), sourceNode.domain ?? undefined) }
    }
    const documentPath = sourceNode?.sourcePath ?? (reference.endsWith('.md') ? reference : null)
    if (documentPath) return { label: 'Open document', run: () => onOpenMemory(documentPath) }
    return null
  }

  const nodeRelations = useMemo(() => {
    if (!selectedNode) return []
    return allEdges
      .filter((edge) => (edge.source === selectedNode.id || edge.target === selectedNode.id) && relationIsEnabled(edge))
      .sort((left, right) => Number(right.evidence === 'observed') - Number(left.evidence === 'observed') || right.weight - left.weight)
  }, [allEdges, relationIsEnabled, selectedNode])

  const breadcrumb = selectedActivity && selectedNode
    ? ['Overview', 'Activity', selectedActivity.title, selectedNode.label]
    : selectedNode
      ? ['Overview', selectedNode.groupId ? nodeById.get(selectedNode.groupId)?.label ?? 'Group' : RING_META[selectedNode.ring].label, selectedNode.label]
      : selectedActivity
        ? ['Overview', 'Activity', selectedActivity.title]
      : ['Overview']
  const counts = orbitQuery.data?.counts

  return (
    <div className={`orbit-workspace ${cinema ? 'is-cinema' : ''}`}>
      <section className="surface orbit-toolbar">
        <div className="orbit-title-block"><p className="eyebrow">Second Brain topology</p><h2>3D Brain</h2></div>
        <div className="orbit-mode-switch" aria-label="Map mode">
          <button aria-pressed={mode === 'structure'} className={mode === 'structure' ? 'is-active' : ''} onClick={() => { setMode('structure'); setSelectedTaskId(null) }} type="button"><ListTree aria-hidden="true" size={14} />Structure</button>
          <button aria-pressed={mode === 'activity'} className={mode === 'activity' ? 'is-active' : ''} onClick={() => { setMode('activity'); setSelectedId(null) }} type="button"><Activity aria-hidden="true" size={14} />Activity</button>
        </div>
        <div className="orbit-search-shell">
          <label className="orbit-search"><Search aria-hidden="true" size={16} /><input aria-label="Search all map items" onChange={(event) => changeSearch(event.target.value)} placeholder="Search groups, items, and sources" type="search" value={search} /></label>
          {query && <div className="orbit-search-results" role="listbox" aria-label="Map search results"><div><strong>{matchingSearchResults.length}</strong> result{matchingSearchResults.length === 1 ? '' : 's'} in authorized data{searchResults.length < matchingSearchResults.length ? ` · showing ${searchResults.length}` : ''}</div>{searchResults.map((node) => <button key={node.id} onClick={() => selectNode(node.id, true)} role="option" type="button"><span>{node.label}<small>{kindLabel(node)}{node.domain ? ` · ${DOMAIN_LABELS[node.domain] ?? node.domain}` : ''}</small></span><Crosshair aria-hidden="true" size={14} /></button>)}{searchResults.length < matchingSearchResults.length && <button onClick={() => setSearchLimit((current) => current + SEARCH_PAGE_SIZE)} type="button">Show {Math.min(SEARCH_PAGE_SIZE, matchingSearchResults.length - searchResults.length)} more</button>}{matchingSearchResults.length === 0 && <p>No authorized item matches the active filters.</p>}</div>}
        </div>
        <div className="orbit-toolbar-actions">
          <select aria-label="Filter map by domain" onChange={(event) => setDomain(event.target.value || undefined)} value={domain ?? ''}><option value="">All domains</option>{DOMAINS.map((item) => <option key={item} value={item}>{DOMAIN_LABELS[item]}</option>)}</select>
          {mode === 'activity' && <select aria-label="Activity interval" onChange={(event) => setActivityWindow(event.target.value as OrbitActivityWindow)} value={activityWindow}><option value="today">Today</option><option value="7d">Last 7 days</option></select>}
          <label className="memory-toggle-label orbit-sensitive-toggle"><input checked={includeSensitive} onChange={(event) => setIncludeSensitive(event.target.checked)} type="checkbox" />Sensitive</label>
          <button className="secondary-button" disabled={orbitQuery.isFetching} onClick={() => void orbitQuery.refetch()} type="button"><RefreshCw aria-hidden="true" className={orbitQuery.isFetching ? 'is-spinning' : ''} size={15} />Refresh</button>
        </div>
      </section>

      <div className="orbit-context-bar">
        <nav aria-label="Exploration breadcrumb">{breadcrumb.map((item, index) => <span key={`${item}-${index}`}>{index > 0 && <ChevronRight aria-hidden="true" size={12} />}{item}</span>)}</nav>
        <div className="orbit-context-actions"><button disabled={breadcrumb.length === 1} onClick={goBackLevel} type="button"><ArrowLeft aria-hidden="true" size={14} />Back</button><button onClick={resetOverview} type="button"><RotateCcw aria-hidden="true" size={14} />Reset overview</button><button disabled={!selectedNode} onClick={() => selectedNode && requestFocus(selectedNode.id)} type="button"><Crosshair aria-hidden="true" size={14} />Center selection</button>{!detailOpen && <button onClick={() => setDetailOpen(true)} type="button"><Eye aria-hidden="true" size={14} />Show details</button>}</div>
      </div>

      <div className="orbit-active-filters" aria-label="Active map filters"><span>Mode: {mode}</span>{domain && <span>Domain: {DOMAIN_LABELS[domain]}</span>}{includeSensitive && <span>Sensitive content included</span>}{query && <span>Search: “{search.trim()}” · {matchingSearchResults.length} results</span>}{expandedGroups.size > 0 && <span>{expandedGroups.size} expanded group{expandedGroups.size === 1 ? '' : 's'}</span>}{mode === 'structure' && counts?.routines === 0 && <span role="status">Routines ring empty · no indexed routine</span>}</div>
      {orbitQuery.error && <div className="inline-error" role="alert">{errorMessage(orbitQuery.error)}</div>}

      <div className={`orbit-main ${detailOpen ? '' : 'is-detail-closed'}`}>
        <section className="surface orbit-stage-shell">
          <div className="orbit-ring-legend" aria-label="Brain categories">{RING_META.slice(1).map((ring, index) => { const Icon = ring.icon; const count = [counts?.skills ?? 0, counts?.memories ?? 0, counts?.routines ?? 0, counts?.applications ?? 0][index]; return <span key={ring.label}><Icon aria-hidden="true" size={12} style={{ color: ring.color }} />{ring.label}<strong>{formatCompactNumber(count)}</strong></span> })}<span className="orbit-size-note"><i aria-hidden="true" />Node size reflects group count</span></div>
          <div className="orbit-scene-controls" aria-label="3D brain controls">
            <button aria-pressed={replay.active} disabled={replay.active || visibleNodes.length === 0} onClick={playGrowth} type="button"><Play aria-hidden="true" size={14} />{replay.complete ? 'Replay growth' : 'Play growth'}</button>
            <button aria-pressed={!motionEnabled} disabled={reducedMotion} onClick={() => setMotionEnabled((current) => !current)} type="button">{motionEnabled ? <Pause aria-hidden="true" size={14} /> : <Play aria-hidden="true" size={14} />}{reducedMotion ? 'Reduced motion' : motionEnabled ? 'Pause motion' : 'Resume motion'}</button>
            <button aria-pressed={cinema} onClick={() => setCinema((current) => !current)} type="button">{cinema ? <Minimize2 aria-hidden="true" size={14} /> : <Maximize2 aria-hidden="true" size={14} />}{cinema ? 'Exit cinema' : 'Cinema'}</button>
          </div>
          {mode === 'activity' && <div className="orbit-activity-dock" aria-label="Recorded task activity"><div><strong>Recorded tasks</strong><small>{activityWindow === 'today' ? 'Today' : 'Last 7 days'}</small></div><div className="orbit-activity-list">{orbitQuery.data?.activities.map((activity) => <button aria-pressed={selectedTaskId === activity.taskId} className={selectedTaskId === activity.taskId ? 'is-active' : ''} key={activity.taskId} onClick={() => selectTask(activity)} type="button"><span>{activity.title}<small>{DOMAIN_LABELS[activity.domain] ?? activity.domain} · {stateLabel(activity.status)}</small></span><strong>{activity.telemetryAvailable ? activity.eventCount : 'N/A'}</strong></button>)}{orbitQuery.data?.activities.length === 0 && <p>No recorded task activity in this interval.</p>}</div></div>}
          <OrbitGlobe
            edges={globeEdges}
            highlightedIds={highlightedIds}
            motionEnabled={motionEnabled}
            nodes={visibleNodes}
            onBackgroundClick={() => { setSelectedId(null); setSelectedTaskId(null); setSelectedEdgeId(null); setDetailOpen(false) }}
            onEdgeClick={(id) => { setSelectedEdgeId(id); setDetailOpen(true) }}
            onNodeClick={(id) => selectNode(id)}
            onPerformance={recordGraphPerformance}
            onReplayProgress={setReplay}
            reducedMotion={reducedMotion}
            ref={globeRef}
            renderedEdgeIds={renderedEdgeIds}
            replayNonce={replayNonce}
            selectedEdgeId={selectedEdgeId}
            selectedId={selectedId}
          />
          {(replay.active || replay.complete) && <div className="orbit-growth-caption" aria-live="polite"><span>The growth of Agentic OS</span><strong>{replay.active ? replay.progress < 0.05 ? 'It starts with one idea.' : replay.progress < 0.14 ? 'Then, a connection.' : replay.progress < 0.42 ? 'Ideas become branches.' : replay.progress < 0.76 ? 'Knowledge compounds.' : 'A second brain takes shape.' : 'Your entire brain. Connected.'}</strong><small>{replay.visible} of {replay.total} authorized nodes</small><i aria-label="Growth replay progress" aria-valuemax={100} aria-valuemin={0} aria-valuenow={Math.round(replay.progress * 100)} role="progressbar" style={{ '--orbit-growth': `${replay.progress * 100}%` } as React.CSSProperties} /></div>}
          {orbitQuery.isLoading && <div className="orbit-loading"><Network aria-hidden="true" size={34} /><p>Composing authorized local registries…</p></div>}
          <div className="orbit-evidence-legend" aria-label="Relation evidence legend">{(Object.entries(EVIDENCE_META) as [OrbitEdge['evidence'], (typeof EVIDENCE_META)[OrbitEdge['evidence']]][]).map(([key, meta]) => <span key={key}><i className={`is-${key}`} style={{ borderColor: meta.color }} />{meta.label}<small>{meta.style}</small></span>)}<span className="orbit-distance-note">Position shows category only, never semantic similarity. Drag to orbit · scroll to zoom.</span></div>
          <div className="orbit-relations-control"><button aria-expanded={showRelations} onClick={() => setShowRelations((current) => !current)} type="button">{showRelations ? <EyeOff aria-hidden="true" size={14} /> : <Eye aria-hidden="true" size={14} />}Show relations</button>{showRelations && <div className="orbit-relations-popover"><strong>Advanced relation rendering</strong><fieldset><legend>Evidence</legend>{(['declared', 'observed', 'inferred'] as const).map((evidence) => <label key={evidence}><input checked={evidenceTypes.has(evidence)} onChange={() => setEvidenceTypes((current) => { const next = new Set(current); if (next.has(evidence)) next.delete(evidence); else next.add(evidence); return next })} type="checkbox" />{EVIDENCE_META[evidence].label}</label>)}</fieldset><fieldset><legend>Types</legend>{allRelationTypes.map((relation) => <label key={relation}><input checked={relationTypes.has(relation)} onChange={() => setRelationTypes((current) => { const next = new Set(current); if (next.has(relation)) next.delete(relation); else next.add(relation); return next })} type="checkbox" />{relation}</label>)}</fieldset><label>Rendering limit<select onChange={(event) => setRelationLimit(Number(event.target.value))} value={relationLimit}><option value="60">60 edges</option><option value="120">120 edges</option><option value="240">240 edges</option></select></label><small>{renderedEdges.length} rendered of {allEdges.length} authorized relations.</small></div>}</div>
          <details className="orbit-accessible-list"><summary><ListTree aria-hidden="true" size={14} />Explore as an accessible list</summary><div>{nodes.filter((node) => node.groupId === null && node.kind !== 'core').map((group) => { const children = childrenByGroup.get(group.id) ?? []; const expanded = expandedGroups.has(group.id); const limit = groupLimits[group.id] ?? DEFAULT_CHILD_LIMIT; return <section key={group.id}><div><button onClick={() => selectNode(group.id)} type="button">{group.label} · {group.count}</button><button aria-expanded={expanded} onClick={() => toggleGroup(group.id)} type="button">{expanded ? 'Collapse' : 'Expand'}</button></div>{expanded && <ul>{children.slice(0, limit).map((child) => <li key={child.id}><button onClick={() => selectNode(child.id, true)} type="button">{child.label}<small>{kindLabel(child)}</small></button></li>)}</ul>}{expanded && children.length > limit && <button onClick={() => setGroupLimits((current) => ({ ...current, [group.id]: limit + DEFAULT_CHILD_LIMIT }))} type="button">Show {Math.min(DEFAULT_CHILD_LIMIT, children.length - limit)} more</button>}</section> })}</div></details>
        </section>

        {detailOpen && <aside className="surface orbit-detail" aria-label="Map selection details">
          <button aria-label="Close selection details" className="orbit-detail-close" onClick={() => { setDetailOpen(false); setSelectedId(null); setSelectedTaskId(null); setSelectedEdgeId(null) }} type="button"><X aria-hidden="true" size={15} /></button>
          {selectedEdge ? <>
            <div className="orbit-detail-heading"><Network aria-hidden="true" size={18} /><div><p className="eyebrow">Relation</p><h2>{selectedEdge.relation}</h2></div></div>
            <p className="orbit-preview">{nodeById.get(selectedEdge.source)?.label ?? selectedEdge.source} → {nodeById.get(selectedEdge.target)?.label ?? selectedEdge.target}</p>
            <div className="orbit-detail-badges"><StatusBadge label={EVIDENCE_META[selectedEdge.evidence].label} tone={selectedEdge.evidence === 'observed' ? 'success' : selectedEdge.evidence === 'inferred' ? 'warning' : 'neutral'} /><span>{EVIDENCE_META[selectedEdge.evidence].style}</span><span>{selectedEdge.weight} event{selectedEdge.weight === 1 ? '' : 's'}</span></div>
            <div className="orbit-provenance"><h3>Verifiable evidence</h3>{selectedEdge.provenance.map((item, index) => { const action = evidenceAction(item.reference); return <div key={`${item.reference}-${index}`}><strong>{item.kind}</strong><span>{item.detail}</span>{item.ts && <time>{formatRelativeTime(Date.parse(item.ts))}</time>}{action ? <button onClick={action.run} type="button">{action.label} · {item.reference}</button> : <small>No governed destination is associated with this evidence.</small>}</div> })}</div>
            <div className="orbit-detail-actions"><button className="secondary-button" onClick={() => setSelectedEdgeId(null)} type="button"><ArrowLeft aria-hidden="true" size={14} />Back to item</button></div>
          </> : selectedNode ? <>
            <div className="orbit-detail-heading"><Layers3 aria-hidden="true" size={18} /><div><p className="eyebrow">{kindLabel(selectedNode)}</p><h2>{selectedNode.label}</h2></div></div>
            <p className="orbit-preview">{nodeDescription(selectedNode)}</p>
            {selectedActivity && <p className="orbit-group-label">Inspecting within task: {selectedActivity.title}</p>}
            {(selectedNode.domain || selectedNode.groupId) && <p className="orbit-group-label">{selectedNode.domain ? DOMAIN_LABELS[selectedNode.domain] ?? selectedNode.domain : nodeById.get(selectedNode.groupId ?? '')?.label}</p>}
            <div className="orbit-detail-badges"><StatusBadge label={stateLabel(selectedNode.operationalState)} tone={selectedNode.operationalState === 'attention' || selectedNode.status === 'stale' || selectedNode.connectionState === 'failing' ? 'warning' : selectedNode.operationalState === 'running' || selectedNode.operationalState === 'in_use' || selectedNode.connectionState === 'working' ? 'success' : 'neutral'} />{selectedNode.catalogState !== 'not_applicable' && <span>Catalog: {stateLabel(selectedNode.catalogState)}</span>}{selectedNode.usageState !== 'not_applicable' && <span>Usage: {stateLabel(selectedNode.usageState)}</span>}{selectedNode.connectionState !== 'not_applicable' && <span>Connection: {stateLabel(selectedNode.connectionState)}</span>}{selectedNode.sensitivity && <span><ShieldCheck aria-hidden="true" size={13} />{selectedNode.sensitivity}</span>}</div>
            {selectedNode.lastActivityAt && <dl className="orbit-user-meta"><dt>Last observed activity</dt><dd>{formatRelativeTime(Date.parse(selectedNode.lastActivityAt))}</dd></dl>}
            <div className="orbit-relations"><h3>Main relations <span>{nodeRelations.length}</span></h3>{nodeRelations.slice(0, 18).map((edge) => { const otherId = edge.source === selectedNode.id ? edge.target : edge.source; const other = nodeById.get(otherId); return <div className="orbit-relation-row" key={edge.id}><button onClick={() => setSelectedEdgeId(edge.id)} type="button"><span><strong>{relationLabel(edge, selectedNode.id)}</strong><small>{EVIDENCE_META[edge.evidence].label} · {edge.source === selectedNode.id ? 'outgoing' : 'incoming'} · {edge.weight}</small></span><span>{other?.label ?? otherId}</span></button><button aria-label={`Go to ${other?.label ?? otherId}`} onClick={() => selectNode(otherId, true)} type="button"><ChevronRight aria-hidden="true" size={14} /></button></div> })}{nodeRelations.length === 0 && <p className="row-subtle">No governed relation is recorded for this item with the active relation filters.</p>}</div>
            {selectedNode.aggregate && expandedGroups.has(selectedNode.id) && (() => { const childCount = (childrenByGroup.get(selectedNode.id) ?? []).length; const visibleCount = Math.min(groupLimits[selectedNode.id] ?? DEFAULT_CHILD_LIMIT, childCount); return <div className="orbit-group-progress"><span><strong>{visibleCount}</strong> of {childCount} items shown</span>{visibleCount < childCount && <button className="secondary-button" onClick={() => setGroupLimits((current) => ({ ...current, [selectedNode.id]: visibleCount + DEFAULT_CHILD_LIMIT }))} type="button">Show {Math.min(DEFAULT_CHILD_LIMIT, childCount - visibleCount)} more</button>}</div> })()}
            <div className="orbit-detail-actions">{selectedActivity && <button className="secondary-button" onClick={() => { setSelectedId(null); setSelectedEdgeId(null) }} type="button"><ArrowLeft aria-hidden="true" size={14} />Back to task</button>}{selectedNode.aggregate && <button className="secondary-button" onClick={() => toggleGroup(selectedNode.id)} type="button">{expandedGroups.has(selectedNode.id) ? 'Collapse group' : 'Expand group'}</button>}{selectedNode.actions.includes('open_memory') && selectedNode.sourcePath && <button className="primary-button" onClick={() => onOpenMemory(selectedNode.sourcePath!)} type="button"><ExternalLink aria-hidden="true" size={14} />Open document</button>}{selectedNode.actions.includes('open_source') && onOpenSource && <button className="primary-button" onClick={() => onOpenSource(selectedNode.id.replace(/^source:/, ''), selectedNode.domain ?? undefined)} type="button"><FileText aria-hidden="true" size={14} />Open original source</button>}{catalogId(selectedNode) && <button className="primary-button" onClick={() => navigateCatalog(selectedNode)} type="button"><ExternalLink aria-hidden="true" size={14} />Open in Catalog</button>}{selectedNode.actions.includes('confirm_memory') && <button className="secondary-button" disabled={confirmMutation.isPending} onClick={() => confirmMutation.mutate(selectedNode.id.replace(/^memory:/, ''))} type="button"><CheckCircle2 aria-hidden="true" size={14} />Confirm still true</button>}</div>
            <details className="orbit-technical"><summary>Technical details</summary><dl><dt>ID</dt><dd><code>{selectedNode.id}</code></dd><dt>Source ref</dt><dd><code>{selectedNode.sourceRef}</code></dd>{selectedNode.sourcePath && <><dt>Path</dt><dd><code>{selectedNode.sourcePath}</code></dd></>}<dt>Visible items</dt><dd>{selectedNode.count}</dd><dt>Domains</dt><dd>{selectedNode.domains.map((item) => `${item.value} (${item.evidence})`).join(', ') || 'None declared'}</dd><dt>Capabilities</dt><dd>{selectedNode.capabilities.map((item) => `${item.value} (${item.evidence})`).join(', ') || 'None declared'}</dd></dl></details>
          </> : selectedActivity ? <>
            <div className="orbit-detail-heading"><Activity aria-hidden="true" size={18} /><div><p className="eyebrow">Recorded task</p><h2>{selectedActivity.title}</h2></div></div>
            <p className="orbit-preview">Observed events for this task. A memory “inserted into context” is not claimed to have determined the answer.</p>
            <div className="orbit-detail-badges"><StatusBadge label={stateLabel(selectedActivity.status)} tone={selectedActivity.status === 'completed' ? 'success' : selectedActivity.status === 'failed' ? 'danger' : 'neutral'} /><span>{DOMAIN_LABELS[selectedActivity.domain] ?? selectedActivity.domain}</span></div>
            <dl className="orbit-user-meta"><dt>Last recorded activity</dt><dd>{formatRelativeTime(Date.parse(selectedActivity.updatedAt))}</dd><dt>Telemetry</dt><dd>{selectedActivity.telemetryAvailable ? `${selectedActivity.eventCount} recorded events` : 'Data not available'}</dd></dl>
            <div className="orbit-relations"><h3>Observed links <span>{selectedActivity.links.length}</span></h3>{selectedActivity.links.map((link, index) => <div className="orbit-activity-link" key={`${link.eventRef}-${link.nodeId}-${index}`}><button onClick={() => selectNode(link.nodeId, true)} type="button"><span><strong>{link.relation}</strong><small>{link.detail}</small><small><time dateTime={link.occurredAt}>{formatRelativeTime(Date.parse(link.occurredAt))}</time>{link.outcome ? ` · ${stateLabel(link.outcome)}` : ''}</small></span><span>{nodeById.get(link.nodeId)?.label ?? link.nodeId}</span><ChevronRight aria-hidden="true" size={14} /></button></div>)}{selectedActivity.telemetryAvailable && selectedActivity.links.length === 0 && <p className="row-subtle">No structured memory, skill, routine, or application reference is available for these events.</p>}{!selectedActivity.telemetryAvailable && <p className="orbit-data-unavailable">Data not available: this task has no recorded events in the selected interval.</p>}</div>
            <details className="orbit-technical"><summary>Technical details</summary><dl><dt>Task ID</dt><dd><code>{selectedActivity.taskId}</code></dd><dt>Window</dt><dd>{activityWindow}</dd></dl></details>
          </> : <div className="orbit-overview-detail"><Layers3 aria-hidden="true" size={22} /><h2>System overview</h2><p>Select an item to inspect it. Expansion is a separate action, available in the detail panel and accessible list.</p><dl><dt>Skills</dt><dd>{formatCompactNumber(counts?.skills ?? 0)}</dd><dt>Memory</dt><dd>{formatCompactNumber(counts?.memories ?? 0)}</dd><dt>Routines</dt><dd>{formatCompactNumber(counts?.routines ?? 0)}</dd><dt>Applications</dt><dd>{formatCompactNumber(counts?.applications ?? 0)}</dd></dl></div>}
          <details className="orbit-performance"><summary>Performance measurements</summary><dl><dt>Payload build</dt><dd>{orbitQuery.data?.metrics.composeMs.toFixed(1) ?? '—'} ms</dd><dt>First useful render</dt><dd>{uiMetrics.firstRenderMs.toFixed(1)} ms</dd><dt>Graph update + transition</dt><dd>{uiMetrics.graphBuildMs.toFixed(1)} ms</dd><dt>Search</dt><dd>{uiMetrics.searchMs.toFixed(2)} ms</dd><dt>Expansion response</dt><dd>{uiMetrics.expansionMs.toFixed(1)} ms</dd><dt>Selection response</dt><dd>{uiMetrics.selectionMs.toFixed(1)} ms</dd></dl><small>Measured locally with the authorized payload: {nodes.length} nodes, {allEdges.length} relations, {orbitQuery.data?.metrics.tracesScanned ?? 0} activity records scanned.</small></details>
        </aside>}
      </div>
    </div>
  )
}
