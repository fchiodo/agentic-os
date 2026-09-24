import { describe, expect, it } from 'vitest'
import type { OrbitEdge, OrbitNode } from '@/features/memory/schema'
import { composeGlobe, growthPosition, planGrowth } from '@/features/memory/orbit-globe-model'

function node(id: string, ring: OrbitNode['ring'], groupId: string | null = null): OrbitNode {
  return {
    id,
    kind: ring === 0 ? 'core' : ring === 2 ? 'memory' : 'skill',
    ring,
    label: id,
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
    sourceRef: `fixture:${id}`,
    groupId,
    count: 1,
    preview: null,
    updatedAt: null,
    actions: [],
    aggregate: false,
  }
}

function edge(id: string, source: string, target: string): OrbitEdge {
  return {
    id,
    source,
    target,
    relation: 'contains',
    evidence: 'declared',
    weight: 1,
    activityAt: null,
    provenance: [],
  }
}

describe('composeGlobe', () => {
  it('keeps the core centered and distributes categories on a deterministic sphere', () => {
    const nodes = [node('core:agentic-os', 0), node('skill:a', 1), node('memory:a', 2)]
    const first = composeGlobe(nodes)
    const second = composeGlobe([...nodes, node('skill:b', 1)])

    expect(first.get('core:agentic-os')).toEqual({ x: 0, y: 0, z: 0 })
    expect(second.get('skill:a')).toEqual(first.get('skill:a'))
    expect(Math.hypot(...Object.values(first.get('skill:a')!))).toBeGreaterThan(280)
    expect(first.get('skill:a')).not.toEqual(first.get('memory:a'))
  })
})

describe('planGrowth', () => {
  it('reveals every node through real edges and starts disconnected components separately', () => {
    const nodes = [node('core:agentic-os', 0), node('skill:a', 1), node('skill:b', 1), node('memory:a', 2)]
    const plan = planGrowth(nodes, [edge('one', 'core:agentic-os', 'skill:a'), edge('two', 'skill:a', 'skill:b')])

    expect(plan.records.map((record) => record.nodeId)).toEqual(['core:agentic-os', 'skill:a', 'skill:b', 'memory:a'])
    expect(plan.records[1]?.edgeId).toBe('one')
    expect(plan.records[2]?.parentId).toBe('skill:a')
    expect(plan.records[3]?.parentId).toBeNull()
    expect(plan.duration).toBe(29)
  })

  it('springs a child from its parent and lands on its final globe position', () => {
    const record = {
      nodeId: 'skill:a',
      parentId: 'core:agentic-os',
      edgeId: 'one',
      index: 1,
      born: 1.2,
      home: { x: 300, y: 100, z: -80 },
      origin: { x: 0, y: 0, z: 0 },
    }

    expect(growthPosition(record, 1.2)).toEqual({ x: 0, y: 0, z: 0 })
    expect(growthPosition(record, 29)).toEqual(record.home)
  })
})
