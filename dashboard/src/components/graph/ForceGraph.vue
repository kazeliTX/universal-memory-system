<script setup lang="ts">
import { ref, onMounted, onUnmounted, watch, nextTick } from 'vue'

export interface GraphNode {
  id: string
  label: string
  group?: string
  size?: number
}

export interface GraphLink {
  source: string
  target: string
  weight?: number
  label?: string
}

const props = withDefaults(
  defineProps<{
    nodes: GraphNode[]
    links: GraphLink[]
    width?: number
    height?: number
    centerNodeId?: string
  }>(),
  { width: 800, height: 500 },
)

const emit = defineEmits<{
  nodeClick: [node: GraphNode]
}>()

const canvasRef = ref<HTMLCanvasElement>()

// --- Simulation state ---

interface SimNode {
  id: string
  label: string
  group: string
  size: number
  x: number
  y: number
  vx: number
  vy: number
  pinned: boolean
}

interface SimLink {
  source: string
  target: string
  weight: number
  label: string
}

let simNodes: SimNode[] = []
let simLinks: SimLink[] = []
let animFrame = 0
let hoveredNode: SimNode | null = null
let draggedNode: SimNode | null = null
let isDragging = false
let mouseX = 0
let mouseY = 0

// Colors by group
const GROUP_COLORS: Record<string, string> = {
  Entity: '#18a058',
  Concept: '#58a6ff',
  Relation: '#f0a020',
  tag: '#c084fc',
  center: '#f97316',
  default: '#8b949e',
}

function getColor(group: string): string {
  return GROUP_COLORS[group] ?? GROUP_COLORS['default']!
}

function brighten(hex: string): string {
  // Simple brighten: blend toward white
  const r = parseInt(hex.slice(1, 3), 16)
  const g = parseInt(hex.slice(3, 5), 16)
  const b = parseInt(hex.slice(5, 7), 16)
  const f = 0.4
  const nr = Math.round(r + (255 - r) * f)
  const ng = Math.round(g + (255 - g) * f)
  const nb = Math.round(b + (255 - b) * f)
  return `rgb(${nr},${ng},${nb})`
}

// --- Build simulation from props ---

function buildSimulation() {
  const w = props.width
  const h = props.height

  simNodes = props.nodes.map((n, i) => {
    // Spread nodes in a circle initially
    const angle = (2 * Math.PI * i) / Math.max(props.nodes.length, 1)
    const radius = Math.min(w, h) * 0.3
    return {
      id: n.id,
      label: n.label,
      group: n.id === props.centerNodeId ? 'center' : (n.group ?? 'default'),
      size: n.id === props.centerNodeId ? 10 : (n.size ?? 6),
      x: w / 2 + radius * Math.cos(angle),
      y: h / 2 + radius * Math.sin(angle),
      vx: 0,
      vy: 0,
      pinned: false,
    }
  })

  simLinks = props.links.map((l) => ({
    source: l.source,
    target: l.target,
    weight: l.weight ?? 1,
    label: l.label ?? '',
  }))
}

function findNode(id: string): SimNode | undefined {
  return simNodes.find((n) => n.id === id)
}

// --- Force calculations ---

function simulate() {
  const w = props.width
  const h = props.height
  const cx = w / 2
  const cy = h / 2
  const alpha = 0.3 // simulation strength
  const repulsion = 2000
  const springK = 0.005
  const springRestLen = 80
  const centerGravity = 0.01
  const damping = 0.85

  // Repulsion between all node pairs
  for (let i = 0; i < simNodes.length; i++) {
    for (let j = i + 1; j < simNodes.length; j++) {
      const a = simNodes[i]!
      const b = simNodes[j]!
      let dx = b.x - a.x
      let dy = b.y - a.y
      let dist = Math.sqrt(dx * dx + dy * dy)
      if (dist < 1) dist = 1
      const force = repulsion / (dist * dist)
      const fx = (dx / dist) * force * alpha
      const fy = (dy / dist) * force * alpha
      if (!a.pinned) { a.vx -= fx; a.vy -= fy }
      if (!b.pinned) { b.vx += fx; b.vy += fy }
    }
  }

  // Attraction along links
  for (const link of simLinks) {
    const a = findNode(link.source)
    const b = findNode(link.target)
    if (!a || !b) continue
    let dx = b.x - a.x
    let dy = b.y - a.y
    let dist = Math.sqrt(dx * dx + dy * dy)
    if (dist < 1) dist = 1
    const force = springK * (dist - springRestLen) * (0.5 + link.weight * 0.5) * alpha
    const fx = (dx / dist) * force
    const fy = (dy / dist) * force
    if (!a.pinned) { a.vx += fx; a.vy += fy }
    if (!b.pinned) { b.vx -= fx; b.vy -= fy }
  }

  // Center gravity
  for (const node of simNodes) {
    if (node.pinned) continue
    node.vx += (cx - node.x) * centerGravity * alpha
    node.vy += (cy - node.y) * centerGravity * alpha
  }

  // Integration & damping
  for (const node of simNodes) {
    if (node.pinned) continue
    node.vx *= damping
    node.vy *= damping
    node.x += node.vx
    node.y += node.vy
    // Keep within bounds
    node.x = Math.max(20, Math.min(w - 20, node.x))
    node.y = Math.max(20, Math.min(h - 20, node.y))
  }
}

// --- Rendering ---

function render() {
  const canvas = canvasRef.value
  if (!canvas) return
  const ctx = canvas.getContext('2d')
  if (!ctx) return

  const w = props.width
  const h = props.height

  ctx.clearRect(0, 0, w, h)

  // Background
  ctx.fillStyle = '#0d1117'
  ctx.fillRect(0, 0, w, h)

  // Draw links
  for (const link of simLinks) {
    const a = findNode(link.source)
    const b = findNode(link.target)
    if (!a || !b) continue
    const thickness = Math.max(1, Math.min(4, link.weight * 1.5))
    ctx.beginPath()
    ctx.moveTo(a.x, a.y)
    ctx.lineTo(b.x, b.y)
    ctx.strokeStyle = 'rgba(255,255,255,0.12)'
    ctx.lineWidth = thickness
    ctx.stroke()

    // Draw link label at midpoint if present
    if (link.label) {
      const mx = (a.x + b.x) / 2
      const my = (a.y + b.y) / 2
      ctx.font = '9px monospace'
      ctx.fillStyle = 'rgba(255,255,255,0.35)'
      ctx.textAlign = 'center'
      ctx.fillText(link.label, mx, my - 4)
    }
  }

  // Draw nodes
  for (const node of simNodes) {
    const isHovered = hoveredNode === node
    const isCenter = node.id === props.centerNodeId
    const baseColor = getColor(node.group)
    const color = isHovered ? brighten(baseColor) : baseColor
    const radius = isCenter ? node.size + 2 : node.size

    // Node circle
    ctx.beginPath()
    ctx.arc(node.x, node.y, radius, 0, Math.PI * 2)
    ctx.fillStyle = color
    ctx.fill()

    // Border for center/hovered
    if (isCenter || isHovered) {
      ctx.strokeStyle = '#ffffff'
      ctx.lineWidth = isCenter ? 2.5 : 1.5
      ctx.stroke()
    }

    // Label
    ctx.font = '11px monospace'
    ctx.fillStyle = isHovered ? '#ffffff' : 'rgba(255,255,255,0.75)'
    ctx.textAlign = 'left'
    const labelText =
      node.label.length > 20 ? node.label.slice(0, 18) + '..' : node.label
    ctx.fillText(labelText, node.x + radius + 4, node.y + 4)
  }

  // Tooltip for hovered node
  if (hoveredNode) {
    const tx = hoveredNode.x + hoveredNode.size + 10
    const ty = hoveredNode.y - 12
    ctx.font = '12px monospace'
    const text = `${hoveredNode.label} (${hoveredNode.group})`
    const tw = ctx.measureText(text).width
    ctx.fillStyle = 'rgba(0,0,0,0.8)'
    ctx.fillRect(tx - 4, ty - 12, tw + 8, 18)
    ctx.fillStyle = '#e6edf3'
    ctx.fillText(text, tx, ty)
  }
}

// --- Animation loop ---

function tick() {
  simulate()
  render()
  animFrame = requestAnimationFrame(tick)
}

function startSimulation() {
  buildSimulation()
  if (animFrame) cancelAnimationFrame(animFrame)
  animFrame = requestAnimationFrame(tick)
}

// --- Mouse interaction ---

function nodeAt(x: number, y: number): SimNode | null {
  // Search in reverse for top-most
  for (let i = simNodes.length - 1; i >= 0; i--) {
    const n = simNodes[i]!
    const dx = x - n.x
    const dy = y - n.y
    const hitRadius = Math.max(n.size + 4, 10)
    if (dx * dx + dy * dy <= hitRadius * hitRadius) return n
  }
  return null
}

function getCanvasPos(e: MouseEvent): [number, number] {
  const rect = canvasRef.value!.getBoundingClientRect()
  return [e.clientX - rect.left, e.clientY - rect.top]
}

function handleMouseMove(e: MouseEvent) {
  const [x, y] = getCanvasPos(e)
  mouseX = x
  mouseY = y

  if (isDragging && draggedNode) {
    draggedNode.x = x
    draggedNode.y = y
    draggedNode.vx = 0
    draggedNode.vy = 0
    return
  }

  const node = nodeAt(x, y)
  hoveredNode = node
  if (canvasRef.value) {
    canvasRef.value.style.cursor = node ? 'pointer' : 'grab'
  }
}

function handleMouseDown(e: MouseEvent) {
  const [x, y] = getCanvasPos(e)
  const node = nodeAt(x, y)
  if (node) {
    isDragging = true
    draggedNode = node
    draggedNode.pinned = true
    if (canvasRef.value) canvasRef.value.style.cursor = 'grabbing'
  }
}

function handleMouseUp(_e: MouseEvent) {
  if (draggedNode) {
    draggedNode.pinned = false
  }
  isDragging = false
  draggedNode = null
}

function handleClick(e: MouseEvent) {
  if (isDragging) return
  const [x, y] = getCanvasPos(e)
  const node = nodeAt(x, y)
  if (node) {
    // Find original node from props
    const original = props.nodes.find((n) => n.id === node.id)
    if (original) emit('nodeClick', original)
  }
}

function handleMouseLeave() {
  hoveredNode = null
  isDragging = false
  if (draggedNode) {
    draggedNode.pinned = false
  }
  draggedNode = null
}

// --- Lifecycle ---

watch(
  () => [props.nodes, props.links, props.centerNodeId],
  () => {
    nextTick(() => startSimulation())
  },
  { deep: true },
)

onMounted(() => {
  startSimulation()
})

onUnmounted(() => {
  if (animFrame) cancelAnimationFrame(animFrame)
})
</script>

<template>
  <canvas
    ref="canvasRef"
    :width="width"
    :height="height"
    style="border: 1px solid #333; border-radius: 4px; background: #0d1117; display: block;"
    @mousemove="handleMouseMove"
    @mousedown="handleMouseDown"
    @mouseup="handleMouseUp"
    @click="handleClick"
    @mouseleave="handleMouseLeave"
  />
</template>
