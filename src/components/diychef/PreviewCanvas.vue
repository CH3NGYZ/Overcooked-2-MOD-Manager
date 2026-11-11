<script setup lang="ts">
import { onBeforeUnmount, onMounted, shallowRef, watch } from 'vue'
import { NSpin } from 'naive-ui'
import { t } from '../../i18n'
import { backendMessageOf } from '../../locale/backend'
import { api } from '../../services/tauri'
import { DEFAULT_PITCH, DEFAULT_YAW } from '../../diychef/preview/rendererPreset'
import { enqueuePreviewMetadata, reservePreviewRenderer } from '../../diychef/preview/queue'
import type { ChefRenderer } from '../../diychef/preview/renderer'
import type { PreparedChef } from '../../diychef/preview/scene'

const props = defineProps<{ sessionId: string; chefId: string; hatId: string }>()
const emit = defineEmits<{ state: [value: 'loading' | 'ready' | 'partial' | 'error'] }>()
const host = shallowRef<HTMLElement | null>(null)
const state = shallowRef<'loading' | 'ready' | 'partial' | 'error'>('loading')
const detail = shallowRef('')
let renderer: ChefRenderer | null = null
let releaseReservation: (() => void) | null = null
let releaseRuntime: (() => void) | null = null
let generation = 0
let disposed = false
let abort: AbortController | null = null
let yaw = DEFAULT_YAW, pitch = DEFAULT_PITCH
let frame = 0
let dragging: { id: number; x: number; y: number } | null = null
function setState(value: typeof state.value) { state.value = value; emit('state', value) }
function draw() {
  if (frame) return
  frame = requestAnimationFrame(() => {
    frame = 0
    try { renderer?.render(yaw, pitch) }
    catch { setState('error') }
  })
}
function reset() { yaw = DEFAULT_YAW; pitch = DEFAULT_PITCH; draw() }
function down(event: PointerEvent) {
  if (event.button !== 0 || !renderer) return
  dragging = { id: event.pointerId, x: event.clientX, y: event.clientY }
  host.value?.setPointerCapture(event.pointerId)
}
function move(event: PointerEvent) {
  if (!dragging || dragging.id !== event.pointerId) return
  yaw -= (event.clientX - dragging.x) * 0.009
  pitch = Math.max(-0.6, Math.min(0.7, pitch + (event.clientY - dragging.y) * 0.009))
  dragging.x = event.clientX; dragging.y = event.clientY
  draw()
}
function end() {
  if (dragging && host.value?.hasPointerCapture(dragging.id)) host.value.releasePointerCapture(dragging.id)
  dragging = null
}
async function load() {
  const epoch = ++generation
  abort?.abort()
  const control = new AbortController()
  abort = control
  detail.value = ''
  // Keep the last rendered pixels under the loading veil while releasing the
  // old model's textures/geometry before decoding its replacement.
  renderer?.clear(true)
  setState('loading')
  let prepared: PreparedChef | null = null
  try {
    const sessionId = props.sessionId, chefId = props.chefId, hatId = props.hatId
    const scene = await enqueuePreviewMetadata(() => api.diyChefPreview(sessionId, chefId, hatId), control.signal)
    if (epoch !== generation || disposed) return
    detail.value = scene.diagnostics.map(backendMessageOf).join(' · ')
    if (scene.status === 'unavailable' || !scene.meshes.length) { renderer?.clear(); setState('error'); return }
    const [{ prepareChef }, runtime] = await Promise.all([
      import('../../diychef/preview/scene'), import('../../diychef/preview/runtime'),
    ])
    prepared = await prepareChef(sessionId, scene, control.signal)
    if (epoch !== generation || disposed) { prepared.dispose(); return }
    renderer = runtime.chefRenderer()
    releaseRuntime = runtime.releaseChefRenderer
    renderer.setChef(prepared)
    prepared = null // Renderer owns geometry, materials and textures from here.
    if (host.value) host.value.prepend(renderer.canvas)
    reset()
    setState(scene.status === 'partial' ? 'partial' : 'ready')
  } catch (reason) {
    prepared?.dispose()
    if (epoch === generation && !disposed) { renderer?.clear(); detail.value = backendMessageOf(reason); setState('error') }
  }
}
onMounted(async () => {
  const release = await reservePreviewRenderer()
  if (disposed) { release(); return }
  releaseReservation = release
  await load()
})
watch(() => [props.sessionId, props.chefId, props.hatId], () => { if (releaseReservation) void load() })
onBeforeUnmount(() => {
  disposed = true; generation += 1; abort?.abort(); end()
  if (frame) cancelAnimationFrame(frame)
  renderer?.clear()
  renderer?.canvas.remove()
  releaseRuntime?.()
  releaseReservation?.()
})
</script>

<template>
  <div class="chef-preview">
    <div ref="host" class="chef-preview-stage" :aria-busy="state === 'loading'"
      :aria-label="t('diychef.rotateHint')" :title="state === 'error' || state === 'partial' ? detail : undefined" @pointerdown="down" @pointermove="move"
      @pointerup="end" @pointercancel="end" @lostpointercapture="end">
      <div v-if="state === 'loading'" class="chef-preview-loading" @pointerdown.stop>
        <n-spin size="medium" />
      </div>
    </div>
  </div>
</template>

<style scoped>
.chef-preview { display: flex; width: 100%; flex-direction: column; align-items: center; gap: 8px; color: var(--oc2-text-2); font-size: 12px; }
.chef-preview-stage { position: relative; width: 100%; aspect-ratio: 4 / 5; touch-action: none; cursor: grab; border-radius: 16px; background: var(--oc2-sunken); overflow: hidden; }
.chef-preview-stage:active { cursor: grabbing; }
.chef-preview-loading { position: absolute; inset: 0; z-index: 1; display: grid; place-items: center; cursor: progress; background: color-mix(in srgb, var(--oc2-sunken) 74%, transparent); }
.chef-preview-stage :deep(canvas) { width: 100%; height: 100%; object-fit: contain; display: block; }
</style>
