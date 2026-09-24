import type { OrbitNode } from '@/features/memory/schema'

const DOMAINS = ['work', 'planphysique', 'personal', 'family', 'finance', 'research'] as const
export const ORBIT_RING_RADII = [0, 20, 39, 58, 78] as const
const RING_PHASE = [0, -Math.PI / 2, 0, -Math.PI / 2, -Math.PI / 2] as const

function stableHash(value: string): number {
  let hash = 2166136261
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  // FNV is fast, but similarly prefixed catalog identifiers can retain visible
  // clustering. An avalanche step keeps the ordering deterministic while
  // distributing those identifiers around the complete ring.
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

function memoryDomainAngle(node: OrbitNode): number | null {
  if (node.kind === 'memory_domain' && node.domain) {
    const domainIndex = Math.max(0, DOMAINS.indexOf(node.domain as (typeof DOMAINS)[number]))
    return -Math.PI / 2 + (Math.PI * 2 * domainIndex) / DOMAINS.length
  }
  return null
}

/** Deterministic category layout. Distances encode rings only, never similarity. */
export function radialPositions(nodes: OrbitNode[]): Map<string, { x: number; y: number }> {
  const positions = new Map<string, { x: number; y: number }>()
  const groupAngles = new Map<string, number>()
  positions.set('core:agentic-os', { x: 0, y: 0 })

  for (const ring of [1, 2, 3, 4] as const) {
    const groups = nodes
      .filter((node) => node.ring === ring && node.groupId === null)
      .sort((left, right) => stableHash(left.id) - stableHash(right.id) || left.id.localeCompare(right.id))

    for (const [index, node] of groups.entries()) {
      const fixedDomainAngle = memoryDomainAngle(node)
      const angle = fixedDomainAngle ?? RING_PHASE[ring] + (Math.PI * 2 * index) / Math.max(1, groups.length)
      const radius = ORBIT_RING_RADII[ring]
      positions.set(node.id, { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius })
      groupAngles.set(node.id, angle)
    }
  }

  const childrenByGroup = new Map<string, OrbitNode[]>()
  for (const node of nodes) {
    if (!node.groupId) continue
    const children = childrenByGroup.get(node.groupId) ?? []
    children.push(node)
    childrenByGroup.set(node.groupId, children)
  }

  for (const [groupId, children] of childrenByGroup) {
    const group = nodes.find((node) => node.id === groupId)
    const ring = group?.ring ?? children[0]?.ring ?? 1
    const baseAngle = groupAngles.get(groupId) ?? RING_PHASE[ring]

    for (const node of children) {
      const angle = baseAngle + (stableUnit(`child-angle:${node.id}`) - 0.5) * 0.92
      const lane = stableHash(`child-lane:${node.id}`) % 4
      const radius = ORBIT_RING_RADII[node.ring] + [-6.3, -2.1, 2.1, 6.3][lane]
      positions.set(node.id, { x: Math.cos(angle) * radius, y: Math.sin(angle) * radius })
    }
  }
  return positions
}
