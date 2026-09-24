import ForceGraph3D, { type ForceGraph3DInstance, type LinkObject, type NodeObject } from '3d-force-graph'
import { forwardRef, useEffect, useImperativeHandle, useRef } from 'react'
import * as THREE from 'three'
import type { OrbitEdge, OrbitNode } from '@/features/memory/schema'
import { composeGlobe, growthPosition, planGrowth } from '@/features/memory/orbit-globe-model'

const CATEGORY_COLORS = ['#ef835d', '#6fa8dc', '#83a866', '#a788c0'] as const
const DOMAIN_COLORS: Record<string, string> = {
  work: '#6fa8dc',
  planphysique: '#69c6a8',
  personal: '#c994e8',
  family: '#e7b36a',
  finance: '#e47777',
  research: '#82a7ef',
}

interface GlobeNode extends NodeObject {
  id: string
  label: string
  model: OrbitNode
  color: string
  radius: number
  forceLabel: boolean
  fx: number
  fy: number
  fz: number
}

interface GlobeLink extends LinkObject<GlobeNode> {
  id: string
  source: string | GlobeNode
  target: string | GlobeNode
  edge: OrbitEdge
  rendered: boolean
}

interface OrbitControlsLike {
  autoRotate: boolean
  autoRotateSpeed: number
  enableDamping: boolean
  dampingFactor: number
  minDistance: number
  maxDistance: number
  addEventListener: (event: string, handler: () => void) => void
}

interface ReleasableRenderer {
  domElement?: HTMLElement
  forceContextLoss?: () => void
}

export interface OrbitGlobeHandle {
  focusNode: (id: string) => void
  resetView: () => void
}

export interface ReplayProgress {
  active: boolean
  complete: boolean
  progress: number
  visible: number
  total: number
}

export interface OrbitGlobeProps {
  nodes: OrbitNode[]
  edges: OrbitEdge[]
  renderedEdgeIds: Set<string>
  selectedId: string | null
  selectedEdgeId: string | null
  highlightedIds: Set<string>
  motionEnabled: boolean
  reducedMotion: boolean
  replayNonce: number
  onBackgroundClick: () => void
  onEdgeClick: (id: string) => void
  onNodeClick: (id: string) => void
  onPerformance: (milliseconds: number) => void
  onReplayProgress: (progress: ReplayProgress) => void
}

function nodeRadius(node: OrbitNode): number {
  if (node.kind === 'core') return 20
  if (node.aggregate) return Math.min(14, 7 + Math.log2(node.count + 1) * 1.35)
  if (node.kind === 'source') return 4.2
  return node.kind === 'memory' ? 5 : 5.7
}

function nodeColor(node: OrbitNode): string {
  if (node.ring === 2 && node.domain) return DOMAIN_COLORS[node.domain] ?? CATEGORY_COLORS[1]
  if (node.ring === 0) return '#8deaff'
  return CATEGORY_COLORS[Math.max(0, node.ring - 1)] ?? '#f4f1e9'
}

function endpointId(value: string | GlobeNode): string {
  return typeof value === 'string' ? value : value.id
}

const sphereGeometry = new THREE.SphereGeometry(1, 18, 14)
const haloGeometry = new THREE.SphereGeometry(1, 14, 10)
const coreGeometry = new THREE.IcosahedronGeometry(1, 1)
const labelCache = new Map<string, THREE.Sprite>()

function clearLabelCache() {
  for (const sprite of labelCache.values()) {
    const material = sprite.material as THREE.SpriteMaterial
    material.map?.dispose()
    material.dispose()
  }
  labelCache.clear()
}

function labelSprite(text: string, color: string, emphasized: boolean): THREE.Sprite {
  const key = `${text}\u0000${color}\u0000${emphasized}`
  const cached = labelCache.get(key)
  if (cached) return cached.clone()
  const canvas = document.createElement('canvas')
  const context = canvas.getContext('2d')
  const fontSize = emphasized ? 30 : 24
  const label = text.length > 30 ? `${text.slice(0, 28).trimEnd()}…` : text
  canvas.width = Math.max(180, Math.min(560, label.length * fontSize * 0.68 + 36))
  canvas.height = 64
  if (context) {
    context.font = `${emphasized ? 650 : 550} ${fontSize}px system-ui, sans-serif`
    context.textAlign = 'center'
    context.textBaseline = 'middle'
    context.shadowColor = '#000'
    context.shadowBlur = 10
    context.fillStyle = '#ffffff'
    context.fillText(label, canvas.width / 2, canvas.height / 2)
  }
  const material = new THREE.SpriteMaterial({
    map: new THREE.CanvasTexture(canvas),
    color,
    depthWrite: false,
    transparent: true,
  })
  const sprite = new THREE.Sprite(material)
  const width = canvas.width / (emphasized ? 4.5 : 5.5)
  sprite.scale.set(width, canvas.height / (emphasized ? 4.5 : 5.5), 1)
  labelCache.set(key, sprite)
  return sprite.clone()
}

function makeNodeObject(node: GlobeNode, selectedId: string | null, highlightedIds: Set<string>): THREE.Object3D {
  const group = new THREE.Group()
  const selected = node.id === selectedId
  const highlighted = highlightedIds.has(node.id)
  const dimmed = highlightedIds.size > 0 && !selected && !highlighted
  const color = new THREE.Color(node.color)

  if (node.model.kind === 'core') {
    const wire = new THREE.Mesh(
      coreGeometry,
      new THREE.MeshBasicMaterial({ color, wireframe: true, transparent: true, opacity: 0.28, depthWrite: false }),
    )
    wire.scale.setScalar(44)
    const hitArea = new THREE.Mesh(
      sphereGeometry,
      new THREE.MeshBasicMaterial({ color, transparent: true, opacity: 0.2 }),
    )
    hitArea.scale.setScalar(30)
    group.add(wire, hitArea)
  } else {
    const radius = node.radius * (selected ? 1.45 : highlighted ? 1.14 : 1)
    const mesh = new THREE.Mesh(
      sphereGeometry,
      new THREE.MeshBasicMaterial({ color, transparent: true, opacity: dimmed ? 0.16 : 0.94 }),
    )
    mesh.scale.setScalar(radius)
    const halo = new THREE.Mesh(
      haloGeometry,
      new THREE.MeshBasicMaterial({ color, transparent: true, opacity: dimmed ? 0 : selected ? 0.22 : 0.07, depthWrite: false }),
    )
    halo.scale.setScalar(radius * 1.55)
    group.add(mesh, halo)
  }

  if (node.forceLabel || selected || highlighted) {
    const label = labelSprite(node.label, selected ? '#ffffff' : node.color, selected)
    label.position.y = node.radius + 18
    group.add(label)
  }
  return group
}

function makeGlowSprite(): THREE.Sprite {
  const canvas = document.createElement('canvas')
  canvas.width = canvas.height = 128
  const context = canvas.getContext('2d')
  if (context) {
    const glow = context.createRadialGradient(64, 64, 0, 64, 64, 64)
    glow.addColorStop(0, 'rgba(120,220,255,.48)')
    glow.addColorStop(0.22, 'rgba(50,150,230,.14)')
    glow.addColorStop(1, 'rgba(20,60,100,0)')
    context.fillStyle = glow
    context.fillRect(0, 0, 128, 128)
  }
  const sprite = new THREE.Sprite(new THREE.SpriteMaterial({
    map: new THREE.CanvasTexture(canvas),
    transparent: true,
    depthWrite: false,
    blending: THREE.AdditiveBlending,
  }))
  sprite.scale.set(330, 330, 1)
  return sprite
}

function makeSceneDressing(scene: THREE.Scene) {
  const group = new THREE.Group()
  const rings: Array<{ line: THREE.Line; bead: THREE.Mesh; radius: number }> = []
  for (let index = 0; index < 2; index += 1) {
    const radius = 430 + index * 42
    const points = Array.from({ length: 241 }, (_, pointIndex) => {
      const angle = (pointIndex / 240) * Math.PI * 2
      return new THREE.Vector3(Math.cos(angle) * radius, Math.sin(angle) * radius, 0)
    })
    const line = new THREE.Line(
      new THREE.BufferGeometry().setFromPoints(points),
      new THREE.LineBasicMaterial({ color: index ? 0x67e8f9 : 0x91aaff, transparent: true, opacity: 0.12, depthWrite: false }),
    )
    line.rotation.set(index ? 0.9 : -0.5, index ? -0.45 : 0.4, 0.2)
    const bead = new THREE.Mesh(new THREE.SphereGeometry(2.8, 10, 8), new THREE.MeshBasicMaterial({ color: 0xb9f5ff }))
    line.add(bead)
    group.add(line)
    rings.push({ line, bead, radius })
  }

  const positions = new Float32Array(700 * 3)
  for (let index = 0; index < 700; index += 1) {
    const radius = 1200 + (index % 97) * 17
    const theta = stableSceneUnit(`star-theta:${index}`) * Math.PI * 2
    const phi = Math.acos(stableSceneUnit(`star-phi:${index}`) * 2 - 1)
    positions[index * 3] = radius * Math.sin(phi) * Math.cos(theta)
    positions[index * 3 + 1] = radius * Math.sin(phi) * Math.sin(theta)
    positions[index * 3 + 2] = radius * Math.cos(phi)
  }
  const starsGeometry = new THREE.BufferGeometry()
  starsGeometry.setAttribute('position', new THREE.BufferAttribute(positions, 3))
  group.add(new THREE.Points(starsGeometry, new THREE.PointsMaterial({ color: 0x9fb8ff, size: 1.35, transparent: true, opacity: 0.28, depthWrite: false })))
  group.add(makeGlowSprite())
  scene.add(group)

  return {
    group,
    update(time: number, reveal: number, motionEnabled: boolean) {
      for (const [index, { line, bead, radius }] of rings.entries()) {
        line.visible = reveal > 0.01
        line.scale.setScalar(0.2 + 0.8 * Math.min(1, reveal))
        if (motionEnabled) line.rotation.z = time * (index ? -0.018 : 0.012)
        bead.position.set(Math.cos(time * 0.16 + index * 2) * radius, Math.sin(time * 0.16 + index * 2) * radius, 0)
        ;(line.material as THREE.LineBasicMaterial).opacity = 0.12 * Math.min(1, reveal)
      }
    },
  }
}

function stableSceneUnit(value: string): number {
  let hash = 2166136261
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index)
    hash = Math.imul(hash, 16777619)
  }
  return (hash >>> 0) / 0x1_0000_0000
}

function cameraDistance(width: number): number {
  return width < 700 ? 1240 : 980
}

export const OrbitGlobe = forwardRef<OrbitGlobeHandle, OrbitGlobeProps>(function OrbitGlobe({
  nodes,
  edges,
  renderedEdgeIds,
  selectedId,
  selectedEdgeId,
  highlightedIds,
  motionEnabled,
  reducedMotion,
  replayNonce,
  onBackgroundClick,
  onEdgeClick,
  onNodeClick,
  onPerformance,
  onReplayProgress,
}, ref) {
  const containerRef = useRef<HTMLDivElement | null>(null)
  const graphRef = useRef<ForceGraph3DInstance<GlobeNode, GlobeLink> | null>(null)
  const dataRef = useRef<{ nodes: GlobeNode[]; links: GlobeLink[] }>({ nodes: [], links: [] })
  const replayFrameRef = useRef<number | null>(null)
  const replayNonceRef = useRef(0)
  const sceneFrameRef = useRef<number | null>(null)
  const sceneTimeRef = useRef(0)
  const lastSceneFrameRef = useRef<number | null>(null)
  const motionRef = useRef(motionEnabled)
  const reducedMotionRef = useRef(reducedMotion)
  const replayActiveRef = useRef(false)
  const growthCameraManualRef = useRef(false)
  const visualStateRef = useRef({ highlightedIds, selectedEdgeId, selectedId })
  const callbacksRef = useRef({ onBackgroundClick, onEdgeClick, onNodeClick, onPerformance, onReplayProgress })
  callbacksRef.current = { onBackgroundClick, onEdgeClick, onNodeClick, onPerformance, onReplayProgress }
  motionRef.current = motionEnabled
  reducedMotionRef.current = reducedMotion
  visualStateRef.current = { highlightedIds, selectedEdgeId, selectedId }

  useImperativeHandle(ref, () => ({
    focusNode(id: string) {
      const graph = graphRef.current
      const node = dataRef.current.nodes.find((candidate) => candidate.id === id)
      if (!graph || !node) return
      const distance = Math.hypot(node.x ?? node.fx, node.y ?? node.fy, node.z ?? node.fz) || 1
      const ratio = 1 + 145 / distance
      graph.cameraPosition(
        { x: (node.x ?? node.fx) * ratio, y: (node.y ?? node.fy) * ratio, z: (node.z ?? node.fz) * ratio },
        { x: node.x ?? node.fx, y: node.y ?? node.fy, z: node.z ?? node.fz },
        reducedMotion ? 0 : 420,
      )
    },
    resetView() {
      const graph = graphRef.current
      const width = containerRef.current?.clientWidth ?? 900
      graph?.cameraPosition({ x: 0, y: 55, z: cameraDistance(width) }, { x: 0, y: 0, z: 0 }, reducedMotion ? 0 : 500)
    },
  }), [reducedMotion])

  useEffect(() => {
    const container = containerRef.current
    if (!container) return
    const graph = new ForceGraph3D(container, {
      controlType: 'orbit',
      rendererConfig: { antialias: true, alpha: false, powerPreference: 'high-performance' },
    }) as ForceGraph3DInstance<GlobeNode, GlobeLink>
    graphRef.current = graph
    graph
      .backgroundColor('#020409')
      .showNavInfo(false)
      .nodeLabel((node) => node.label)
      .nodeThreeObject((node) => makeNodeObject(node, visualStateRef.current.selectedId, visualStateRef.current.highlightedIds))
      .nodeThreeObjectExtend(false)
      .linkColor((link) => link.id === visualStateRef.current.selectedEdgeId ? '#ffffff' : link.edge.evidence === 'observed' ? 'rgba(255,190,158,.65)' : `rgba(139,183,224,${link.rendered ? 0.34 : 0.10})`)
      .linkWidth((link) => link.id === visualStateRef.current.selectedEdgeId ? 2.2 : link.edge.evidence === 'observed' ? 1.2 : 0.45)
      .linkOpacity(1)
      .linkDirectionalParticles((link) => motionRef.current && (link.id === visualStateRef.current.selectedEdgeId || link.edge.evidence === 'observed') ? 2 : 0)
      .linkDirectionalParticleWidth(1.5)
      .linkDirectionalParticleSpeed(0.005)
      .linkDirectionalParticleColor(() => '#ffffff')
      .linkVisibility((link) => link.rendered)
      .onNodeClick((node) => { if (!replayActiveRef.current) callbacksRef.current.onNodeClick(node.id) })
      .onLinkClick((link) => { if (!replayActiveRef.current) callbacksRef.current.onEdgeClick(link.id) })
      .onBackgroundClick(() => { if (!replayActiveRef.current) callbacksRef.current.onBackgroundClick() })
      .enableNodeDrag(false)
      .warmupTicks(0)
      .cooldownTicks(0)

    graph.d3Force('charge', null)
    graph.d3Force('center', null)
    const linkForce = graph.d3Force('link') as { strength?: (value: number) => void } | undefined
    linkForce?.strength?.(0)

    const controls = graph.controls() as OrbitControlsLike
    controls.enableDamping = true
    controls.dampingFactor = 0.08
    controls.autoRotate = motionRef.current && !reducedMotionRef.current
    controls.autoRotateSpeed = 0.42
    controls.minDistance = 60
    controls.maxDistance = 2400
    controls.addEventListener('start', () => {
      controls.autoRotate = false
      if (replayActiveRef.current) growthCameraManualRef.current = true
    })
    controls.addEventListener('end', () => { controls.autoRotate = motionRef.current && !reducedMotionRef.current })

    graph.renderer().setPixelRatio(Math.min(window.devicePixelRatio, 1.6))
    graph.renderer().outputColorSpace = THREE.SRGBColorSpace
    graph.scene().fog = new THREE.FogExp2(0x05070d, 0.00017)
    const dressing = makeSceneDressing(graph.scene())

    const resize = () => {
      const width = Math.max(container.clientWidth, 320)
      const height = Math.max(container.clientHeight, 420)
      graph.width(width).height(height)
    }
    resize()
    const observer = new ResizeObserver(resize)
    observer.observe(container)
    graph.cameraPosition({ x: 0, y: 75, z: cameraDistance(container.clientWidth) * 1.2 })
    const cameraTransitionTimeout = window.setTimeout(
      () => graph.cameraPosition({ x: 0, y: 55, z: cameraDistance(container.clientWidth) }, { x: 0, y: 0, z: 0 }, reducedMotionRef.current ? 0 : 1500),
      120,
    )

    const animateScene = (now: number) => {
      const previous = lastSceneFrameRef.current ?? now
      if (motionRef.current && !reducedMotionRef.current && !document.hidden) sceneTimeRef.current += Math.min((now - previous) / 1000, 0.05)
      lastSceneFrameRef.current = now
      dressing.update(sceneTimeRef.current, replayActiveRef.current ? 0.25 : 1, motionRef.current && !reducedMotionRef.current)
      sceneFrameRef.current = window.requestAnimationFrame(animateScene)
    }
    sceneFrameRef.current = window.requestAnimationFrame(animateScene)

    return () => {
      observer.disconnect()
      window.clearTimeout(cameraTransitionTimeout)
      if (sceneFrameRef.current !== null) window.cancelAnimationFrame(sceneFrameRef.current)
      if (replayFrameRef.current !== null) window.cancelAnimationFrame(replayFrameRef.current)
      sceneFrameRef.current = null
      replayFrameRef.current = null
      replayActiveRef.current = false
      const renderer = graph.renderer() as unknown as ReleasableRenderer
      graph._destructor()
      // three-render-objects disposes the renderer but does not explicitly
      // release WebKit's WebGL context. Rapid route changes can otherwise
      // leave several GPU contexts pending collection and exhaust the macOS
      // WebContent process before GC catches up.
      renderer.forceContextLoss?.()
      renderer.domElement?.remove()
      container.replaceChildren()
      clearLabelCache()
      graphRef.current = null
    }
  }, [])

  useEffect(() => {
    const graph = graphRef.current
    if (!graph) return
    const started = performance.now()
    const positions = composeGlobe(nodes)
    const globeNodes = nodes.map((model) => {
      const point = positions.get(model.id) ?? { x: 0, y: 0, z: 0 }
      return {
        id: model.id,
        label: model.label,
        model,
        color: nodeColor(model),
        radius: nodeRadius(model),
        forceLabel: model.kind === 'core' || model.groupId === null,
        ...point,
        fx: point.x,
        fy: point.y,
        fz: point.z,
      } satisfies GlobeNode
    })
    const globeLinks = edges.map((edge) => ({
      id: edge.id,
      source: edge.source,
      target: edge.target,
      edge,
      rendered: renderedEdgeIds.has(edge.id),
    } satisfies GlobeLink))
    dataRef.current = { nodes: globeNodes, links: globeLinks }
    graph
      .nodeThreeObject((node) => makeNodeObject(node, selectedId, highlightedIds))
      .linkColor((link) => link.id === selectedEdgeId ? '#ffffff' : link.edge.evidence === 'observed' ? 'rgba(255,190,158,.65)' : `rgba(139,183,224,${link.rendered ? 0.34 : 0.10})`)
      .linkWidth((link) => link.id === selectedEdgeId ? 2.2 : link.edge.evidence === 'observed' ? 1.2 : 0.45)
      .linkVisibility((link) => link.rendered)
      .graphData({ nodes: globeNodes, links: globeLinks })
      .refresh()
    callbacksRef.current.onPerformance(performance.now() - started)
  }, [edges, highlightedIds, nodes, renderedEdgeIds, selectedEdgeId, selectedId])

  useEffect(() => {
    const graph = graphRef.current
    const controls = graph?.controls() as OrbitControlsLike | undefined
    if (!graph || !controls) return
    controls.autoRotate = motionEnabled && !reducedMotion
    graph.linkDirectionalParticles((link) => motionEnabled && !reducedMotion && (link.id === selectedEdgeId || link.edge.evidence === 'observed') ? 2 : 0).refresh()
  }, [motionEnabled, reducedMotion, selectedEdgeId])

  useEffect(() => {
    if (replayNonce === 0 || replayNonce === replayNonceRef.current) return
    replayNonceRef.current = replayNonce
    const graph = graphRef.current
    if (!graph || dataRef.current.nodes.length === 0) return
    if (replayFrameRef.current !== null) window.cancelAnimationFrame(replayFrameRef.current)
    const modelNodes = dataRef.current.nodes.map((node) => node.model)
    const modelEdges = dataRef.current.links.map((link) => link.edge)
    const plan = planGrowth(modelNodes, modelEdges)
    const byId = new Map(dataRef.current.nodes.map((node) => [node.id, node]))
    const recordById = new Map(plan.records.map((record) => [record.nodeId, record]))
    const started = performance.now()
    let lastPaint = -1
    const capturedOrigins = new Map<string, { x: number; y: number; z: number }>()
    replayActiveRef.current = true
    growthCameraManualRef.current = false
    for (const node of dataRef.current.nodes) Object.assign(node, { x: 0, y: 0, z: 0, fx: 0, fy: 0, fz: 0 })
    graph
      .nodeVisibility((node) => (recordById.get(node.id)?.born ?? Number.POSITIVE_INFINITY) === 0)
      .linkVisibility(() => false)
      .refresh()
    graph.cameraPosition({ x: 0, y: 25, z: cameraDistance(containerRef.current?.clientWidth ?? 900) * 0.56 }, { x: 0, y: 0, z: 0 }, 0)
    callbacksRef.current.onReplayProgress({ active: true, complete: false, progress: 0, visible: 1, total: plan.records.length })

    const finish = () => {
      for (const record of plan.records) {
        const node = byId.get(record.nodeId)
        if (!node) continue
        Object.assign(node, record.home, { fx: record.home.x, fy: record.home.y, fz: record.home.z })
      }
      replayActiveRef.current = false
      graph.nodeVisibility(() => true).linkVisibility((link) => link.rendered).refresh()
      callbacksRef.current.onReplayProgress({ active: false, complete: true, progress: 1, visible: plan.records.length, total: plan.records.length })
      replayFrameRef.current = null
    }

    if (reducedMotion) {
      finish()
      return
    }

    const animate = (now: number) => {
      const time = Math.min(plan.duration, (now - started) / 1000)
      for (const record of plan.records) {
        const node = byId.get(record.nodeId)
        if (!node) continue
        if (time >= record.born && !capturedOrigins.has(record.nodeId)) {
          const parent = record.parentId ? byId.get(record.parentId) : null
          capturedOrigins.set(record.nodeId, parent ? { x: parent.x ?? 0, y: parent.y ?? 0, z: parent.z ?? 0 } : { x: 0, y: 0, z: 0 })
        }
        const point = growthPosition({ ...record, origin: capturedOrigins.get(record.nodeId) ?? record.origin }, time)
        Object.assign(node, point, { fx: point.x, fy: point.y, fz: point.z })
      }
      if (!growthCameraManualRef.current) {
        const distance = cameraDistance(containerRef.current?.clientWidth ?? 900) * (0.56 + 0.44 * Math.min(1, time / 26))
        graph.camera().position.setLength(distance)
      }
      if (time - lastPaint >= 0.1 || time >= plan.duration) {
        lastPaint = time
        const visible = plan.records.filter((record) => record.born <= time).length
        graph
          .nodeVisibility((node) => (recordById.get(node.id)?.born ?? Number.POSITIVE_INFINITY) <= time)
          .linkVisibility((link) => {
            const sourceBorn = recordById.get(endpointId(link.source))?.born ?? Number.POSITIVE_INFINITY
            const targetBorn = recordById.get(endpointId(link.target))?.born ?? Number.POSITIVE_INFINITY
            return Math.max(sourceBorn, targetBorn) <= time && (plan.treeEdgeIds.has(link.id) || (time > 13 && link.rendered))
          })
          .refresh()
        callbacksRef.current.onReplayProgress({ active: true, complete: false, progress: time / plan.duration, visible, total: plan.records.length })
      }
      if (time >= plan.duration) finish()
      else replayFrameRef.current = window.requestAnimationFrame(animate)
    }
    replayFrameRef.current = window.requestAnimationFrame(animate)
    return () => {
      if (replayFrameRef.current !== null) window.cancelAnimationFrame(replayFrameRef.current)
      replayFrameRef.current = null
      replayActiveRef.current = false
    }
  }, [reducedMotion, replayNonce])

  return <div aria-label="Interactive 3D brain graph" className="orbit-canvas orbit-globe" ref={containerRef} />
})
