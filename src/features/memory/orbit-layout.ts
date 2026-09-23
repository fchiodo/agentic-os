import type { OrbitNode } from '@/features/memory/schema'

const DOMAINS = ['work', 'planphysique', 'personal', 'family', 'finance', 'research'] as const
const RING_RADII = [0, 13, 25, 37, 49] as const
const RING_PHASE = [0, -0.45, 0, 0.72, 1.38] as const

function stableHash(value: string): number {
  let hash = 2166136261
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  return hash >>> 0
}

function stableUnit(value: string): number {
  return (stableHash(value) % 10_000) / 10_000
}

function angleForTopLevel(node: OrbitNode): number {
  if (node.kind === 'memory_domain' && node.domain) {
    const domainIndex = Math.max(0, DOMAINS.indexOf(node.domain as (typeof DOMAINS)[number]))
    return -Math.PI / 2 + (Math.PI * 2 * domainIndex) / DOMAINS.length
  }
  return -Math.PI / 2 + RING_PHASE[node.ring] + stableUnit(`top:${node.id}`) * Math.PI * 2
}

/** Deterministic category layout. Distances encode rings only, never similarity. */
export function radialPositions(nodes: OrbitNode[]): Map<string, { x: number; y: number }> {
  const positions = new Map<string, { x: number; y: number }>()
  const groupAngles = new Map<string, number>()
  positions.set('core:agentic-os', { x: 0, y: 0 })

  for (const node of nodes) {
    if (node.ring === 0 || node.groupId !== null) continue
    const angle = angleForTopLevel(node)
    const radius = RING_RADII[node.ring]
    positions.set(node.id, { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius })
    groupAngles.set(node.id, angle)
  }

  for (const node of nodes) {
    if (!node.groupId) continue
    const baseAngle = groupAngles.get(node.groupId) ?? angleForTopLevel(node)
    const angle = baseAngle + (stableUnit(`child-angle:${node.id}`) - 0.5) * 0.86
    const lane = stableHash(`child-lane:${node.id}`) % 5
    const radius = RING_RADII[node.ring] + 2.4 + lane * 0.82
    positions.set(node.id, { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius })
  }
  return positions
}
