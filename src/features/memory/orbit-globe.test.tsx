import '@testing-library/jest-dom/vitest'
import { act, cleanup, render } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const graphState = vi.hoisted(() => ({
  destroyed: false,
  destructor: vi.fn(),
}))

vi.mock('3d-force-graph', () => {
  const controls = {
    addEventListener: vi.fn(),
    autoRotate: false,
    autoRotateSpeed: 0,
    dampingFactor: 0,
    enableDamping: false,
    maxDistance: 0,
    minDistance: 0,
  }
  const renderer = {
    outputColorSpace: '',
    setPixelRatio: vi.fn(),
  }
  const scene = { add: vi.fn(), fog: null }
  const graph: Record<string, unknown> = {}
  const chain = vi.fn(() => graph)
  Object.assign(graph, {
    _destructor: vi.fn(() => {
      graphState.destroyed = true
      graphState.destructor()
    }),
    backgroundColor: chain,
    cameraPosition: vi.fn(() => {
      if (graphState.destroyed) throw new Error('cameraPosition called after graph destruction')
      return graph
    }),
    controls: vi.fn(() => controls),
    cooldownTicks: chain,
    d3Force: vi.fn((name: string) => name === 'link' ? { strength: vi.fn() } : undefined),
    enableNodeDrag: chain,
    graphData: chain,
    linkColor: chain,
    linkDirectionalParticleColor: chain,
    linkDirectionalParticles: chain,
    linkDirectionalParticleSpeed: chain,
    linkDirectionalParticleWidth: chain,
    linkOpacity: chain,
    linkVisibility: chain,
    linkWidth: chain,
    nodeLabel: chain,
    nodeThreeObject: chain,
    nodeThreeObjectExtend: chain,
    onBackgroundClick: chain,
    onLinkClick: chain,
    onNodeClick: chain,
    refresh: chain,
    renderer: vi.fn(() => renderer),
    scene: vi.fn(() => scene),
    showNavInfo: chain,
    warmupTicks: chain,
    width: chain,
    height: chain,
  })

  return {
    default: function ForceGraph3D() {
      graphState.destroyed = false
      return graph
    },
  }
})

import { OrbitGlobe } from '@/features/memory/orbit-globe'

beforeEach(() => {
  graphState.destroyed = false
  graphState.destructor.mockClear()
  vi.useFakeTimers()
  vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
    createRadialGradient: () => ({ addColorStop: vi.fn() }),
    fillRect: vi.fn(),
    fillStyle: '',
  } as unknown as CanvasRenderingContext2D)
  vi.stubGlobal('ResizeObserver', class {
    disconnect() {}
    observe() {}
  })
  vi.stubGlobal('requestAnimationFrame', vi.fn(() => 1))
  vi.stubGlobal('cancelAnimationFrame', vi.fn())
})

afterEach(() => {
  cleanup()
  vi.useRealTimers()
  vi.restoreAllMocks()
  vi.unstubAllGlobals()
})

describe('OrbitGlobe lifecycle', () => {
  it('does not run the delayed camera transition after unmount', () => {
    const view = render(
      <OrbitGlobe
        edges={[]}
        highlightedIds={new Set()}
        motionEnabled
        nodes={[]}
        onBackgroundClick={vi.fn()}
        onEdgeClick={vi.fn()}
        onNodeClick={vi.fn()}
        onPerformance={vi.fn()}
        onReplayProgress={vi.fn()}
        reducedMotion={false}
        renderedEdgeIds={new Set()}
        replayNonce={0}
        selectedEdgeId={null}
        selectedId={null}
      />,
    )

    view.unmount()

    expect(graphState.destructor).toHaveBeenCalledOnce()
    expect(() => act(() => vi.advanceTimersByTime(120))).not.toThrow()
  })
})
