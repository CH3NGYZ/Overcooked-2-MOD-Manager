import type { ChefPreviewScene } from './types'

const MAX_BYTES = 96 * 1024 * 1024
interface Waiting { bytes: number; start: () => void }
const waiting: Waiting[] = []
let used = 0
let peak = 0

/** Reserve a complete scene before decoding any assets. Incremental per-texture
 * reservations could deadlock several half-prepared scenes waiting for GPU.
 * One scene larger than the budget may run alone, preserving custom models. */
export function reservePreviewMemory(scene: ChefPreviewScene, signal?: AbortSignal): Promise<() => void> {
  if (signal?.aborted) return Promise.reject(new DOMException('Aborted', 'AbortError'))
  const geometry = scene.meshes.reduce((sum, mesh) => sum
    + (Number.isFinite(mesh.vertex_count) ? Math.max(0, mesh.vertex_count) * 32 : 0)
    + (Number.isFinite(mesh.index_count) ? Math.max(0, mesh.index_count) * 4 : 0), 0) * 2
  const textures = new Set(scene.meshes.map(mesh => mesh.texture_asset).filter(Boolean)).size
  // Older scene descriptors have no hint. The backend supplies a decoded pixel
  // estimate for current scenes; this fallback keeps old descriptors usable.
  const hint = scene.estimated_bytes ?? 0
  const bytes = Math.max(64 * 1024, Number.isFinite(hint) && hint > 0
    ? hint : geometry + textures * 16 * 1024 * 1024)
  return new Promise<() => void>((resolve, reject) => {
    const cancel = () => {
      const index = waiting.indexOf(job)
      if (index < 0) return
      waiting.splice(index, 1)
      signal?.removeEventListener('abort', cancel)
      reject(new DOMException('Aborted', 'AbortError'))
      pump()
    }
    const job: Waiting = { bytes, start() {
      signal?.removeEventListener('abort', cancel)
      used += bytes
      peak = Math.max(peak, used)
      let released = false
      resolve(() => {
        if (released) return
        released = true
        used -= bytes
        pump()
      })
    } }
    waiting.push(job)
    signal?.addEventListener('abort', cancel, { once: true })
    pump()
  })
}

function pump() {
  while (waiting.length) {
    const next = waiting[0]!
    if (used && used + next.bytes > MAX_BYTES) return
    waiting.shift()!.start()
  }
}

export function previewMemoryStats() {
  return { bytes: used, peakBytes: peak, queued: waiting.length, budgetBytes: MAX_BYTES }
}
