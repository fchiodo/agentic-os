import type { OrbitEdge, OrbitNode } from '@/features/memory/schema'

export interface GlobePoint {
  x: number
  y: number
  z: number
}

export interface GrowthRecord {
  nodeId: string
  parentId: string | null
  edgeId: string | null
  index: number
  born: number
  home: GlobePoint
  origin: GlobePoint
}

export interface GrowthPlan {
  records: GrowthRecord[]
  treeEdgeIds: Set<string>
  duration: 29
}

const CATEGORY_COUNT = 4
const GOLDEN_RATIO_CONJUGATE = 0.61803398875

function stableHash(value: string): number {
  let hash = 2166136261
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  hash ^= hash >>> 16
  hash = Math.imul(hash, 0x85ebca6b)
  hash ^= hash >>> 13
  hash = Math.imul(hash, 0xc2b2ae35)
  hash ^= hash >>> 16
  return hash >>> 0
}

function stableUnit(value: string): number {
  return stableHash(value) / 0x1_0000_0000
}

/** Deterministic equal-area category sectors adapted from AIS-OS 3d-brain. */
export function composeGlobe(nodes: OrbitNode[]): Map<string, GlobePoint> {
  const positions = new Map<string, GlobePoint>()
  positions.set('core:agentic-os', { x: 0, y: 0, z: 0 })

  for (const node of nodes) {
    if (node.ring === 0) {
      positions.set(node.id, { x: 0, y: 0, z: 0 })
      continue
    }
    const sector = Math.max(0, Math.min(CATEGORY_COUNT - 1, node.ring - 1))
    const y = stableUnit(`globe-y:${node.id}`) * 2 - 1
    const turn = (stableUnit(`globe-turn:${node.id}`) + GOLDEN_RATIO_CONJUGATE) % 1
    const angle = ((sector + 0.08 + turn * 0.84) / CATEGORY_COUNT) * Math.PI * 2
    const radius = 300 + stableUnit(`globe-radius:${node.id}`) * 82
    const horizontal = Math.sqrt(1 - y * y)
    positions.set(node.id, {
      x: radius * horizontal * Math.sin(angle),
      y: radius * y,
      z: radius * horizontal * Math.cos(angle),
    })
  }
  return positions
}

/** Connectivity replay: parent-child records only use edges present in the payload. */
export function planGrowth(nodes: OrbitNode[], edges: OrbitEdge[]): GrowthPlan {
  const positions = composeGlobe(nodes)
  const byId = new Map(nodes.map((node) => [node.id, node]))
  const adjacent = new Map(nodes.map((node) => [node.id, [] as Array<{ nodeId: string; edgeId: string }>]))

  for (const edge of edges) {
    if (!byId.has(edge.source) || !byId.has(edge.target)) continue
    adjacent.get(edge.source)?.push({ nodeId: edge.target, edgeId: edge.id })
    adjacent.get(edge.target)?.push({ nodeId: edge.source, edgeId: edge.id })
  }
  for (const links of adjacent.values()) links.sort((left, right) => left.nodeId.localeCompare(right.nodeId))

  const degree = (id: string) => adjacent.get(id)?.length ?? 0
  const ranked = [...nodes].sort((left, right) => {
    if (left.kind === 'core') return -1
    if (right.kind === 'core') return 1
    return degree(right.id) - degree(left.id) || left.id.localeCompare(right.id)
  })
  const seen = new Set<string>()
  const records: GrowthRecord[] = []
  const treeEdgeIds = new Set<string>()

  const append = (nodeId: string, parentId: string | null, edgeId: string | null) => {
    const home = positions.get(nodeId) ?? { x: 0, y: 0, z: 0 }
    const origin = parentId ? positions.get(parentId) ?? { x: 0, y: 0, z: 0 } : { x: 0, y: 0, z: 0 }
    const record: GrowthRecord = { nodeId, parentId, edgeId, index: records.length, born: 0, home, origin }
    records.push(record)
    seen.add(nodeId)
    if (edgeId) treeEdgeIds.add(edgeId)
    return record
  }

  for (const seed of ranked) {
    if (seen.has(seed.id)) continue
    const queue = [append(seed.id, null, null)]
    for (let index = 0; index < queue.length; index += 1) {
      const parent = queue[index]!
      const available = (adjacent.get(parent.nodeId) ?? []).filter((item) => !seen.has(item.nodeId))
      for (const next of available.slice(0, 3)) queue.push(append(next.nodeId, parent.nodeId, next.edgeId))
      if (available.length > 3) queue.push(parent)
    }
  }

  for (const [index, record] of records.entries()) {
    record.born = index === 0 ? 0 : index === 1 ? 1.2 : index === 2 ? 2.4 : 3 + 23 * Math.pow((index - 2) / Math.max(1, records.length - 3), 0.62)
  }
  return { records, treeEdgeIds, duration: 29 }
}

export function growthPosition(record: GrowthRecord, time: number): GlobePoint {
  if (time >= 29) return record.home
  const age = Math.max(0, time - record.born)
  const unit = Math.min(1, age / 1.6)
  const spring = unit >= 1 ? 1 : 1 - Math.exp(-5 * unit) * Math.cos(7 * unit)
  const spread = 0.17 + 0.83 * Math.min(1, time / 26)
  const progress = record.index === 0 ? Math.min(1, time / 18) : spring
  return {
    x: record.origin.x + (record.home.x * spread - record.origin.x) * progress,
    y: record.origin.y + (record.home.y * spread - record.origin.y) * progress,
    z: record.origin.z + (record.home.z * spread - record.origin.z) * progress,
  }
}
