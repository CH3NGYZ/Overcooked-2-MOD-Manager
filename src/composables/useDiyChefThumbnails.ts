import { api } from '../services/tauri'
import { acquireThumbnail, clearThumbnailCache, putThumbnail, type PreviewThumbnail } from '../diychef/preview/cache'
import { enqueuePreview, enqueuePreviewPreparation } from '../diychef/preview/queue'
import { PREVIEW_VERSION } from '../diychef/preview/rendererPreset'
import { readPreviewMetadata } from '../diychef/preview/metadata'
import { reservePreviewMemory } from '../diychef/preview/memory'
interface Request { promise: Promise<string>; users: Set<symbol>; abort: AbortController; thumbnail?: PreviewThumbnail }
const requests = new Map<string, Request>()
let partialSequence = 0
let cacheEpoch = 0
let clearing: Promise<number> | null = null

/** Cancel old work before the backend clears its matching caches. New requests
 * wait for that operation, so neither side can refill caches while it runs. */
export function clearChefThumbnailCache(): Promise<number> {
  if (clearing) return clearing
  cacheEpoch += 1
  for (const request of requests.values()) {
    request.users.clear()
    request.abort.abort()
    request.thumbnail?.release()
    request.thumbnail = undefined
  }
  requests.clear()
  clearThumbnailCache()
  const task = Promise.resolve().then(() => api.clearDiyChefCache()).finally(() => {
    if (clearing === task) clearing = null
  })
  clearing = task
  return task
}

/** The backend checks only file metadata before returning the cached scene. A
 * fresh editor session does not invalidate identical rendered thumbnails. */
export async function requestChefThumbnail(sessionId: string, chefId: string, hatId: string, signal?: AbortSignal): Promise<PreviewThumbnail> {
  while (clearing) await clearing.catch(() => undefined)
  const epoch = cacheEpoch
  const user = Symbol()
  const requestKey = JSON.stringify([sessionId, chefId, hatId])
  let pending = requests.get(requestKey)
  if (pending?.abort.signal.aborted) pending = undefined
  if (!pending) {
    const request: Request = { promise: Promise.resolve(''), users: new Set(), abort: new AbortController() }
    pending = request
    requests.set(requestKey, request)
    // Register consumers before starting work. Metadata and PNG hits never need
    // the sole renderer, so a cold capture or a dialog must not serialize them.
    request.promise = Promise.resolve().then(async () => {
      const ensureNeeded = () => {
        if (epoch !== cacheEpoch || request.abort.signal.aborted || !request.users.size) throw new DOMException('Aborted', 'AbortError')
      }
      const retain = (key: string) => {
        const thumbnail = acquireThumbnail(key)
        if (!thumbnail) return false
        // Concurrent disk hits can exceed the LRU budget before their awaiting
        // consumers resume. Pin the result until every consumer has its lease.
        request.thumbnail = thumbnail
        return true
      }
      ensureNeeded()
      const fingerprintKey = JSON.stringify([epoch, 'fingerprint', sessionId, chefId, hatId])
      const fingerprint = await readPreviewMetadata(fingerprintKey,
        () => api.diyChefFingerprint(sessionId, chefId, hatId), request.abort.signal)
      ensureNeeded()
      let key = `${PREVIEW_VERSION}:${fingerprint}`
      if (retain(key)) return key
      // Persist only fixed-size rendered PNGs. Cache storage failures should not
      // prevent using an otherwise valid chef or generating a fresh thumbnail.
      const disk = await readPreviewMetadata(JSON.stringify([epoch, 'png', key]),
        () => api.diyChefThumbnail(key), request.abort.signal).catch(() => new ArrayBuffer(0))
      ensureNeeded()
      if (disk.byteLength && new Uint8Array(disk)[0] === 0) {
        putThumbnail(key, new Blob([disk.slice(1)], { type: 'image/png' }), 'ready', [])
        retain(key)
        return key
      }
      return enqueuePreviewPreparation(async () => {
        const [{ prepareChef }, { chefRenderer, releaseChefRenderer }] = await Promise.all([
          import('../diychef/preview/scene'), import('../diychef/preview/runtime'),
        ])
        // The preparation permit remains held while waiting for the sole GPU
        // lane, bounding decoded geometry/textures as well as backend work.
        for (;;) {
          ensureNeeded()
          const scene = await api.diyChefPreview(sessionId, chefId, hatId)
          ensureNeeded()
          if (scene.status === 'unavailable' || !scene.meshes.length) throw new Error(scene.diagnostics[0] ?? 'diychef.previewUnavailable')
          key = `${PREVIEW_VERSION}:${scene.fingerprint}`
          if (retain(key)) return key
          const releaseMemory = await reservePreviewMemory(scene, request.abort.signal)
          let prepared: Awaited<ReturnType<typeof prepareChef>> | undefined
          let rendererOwnsPrepared = false
          let completeImage: Blob | undefined
          try {
            ensureNeeded()
            prepared = await prepareChef(sessionId, scene, request.abort.signal)
            const current = await enqueuePreview(async () => {
              ensureNeeded()
              // Another cold task or the hat dialog may have held the renderer
              // since these assets were prepared. Retry changed resources under
              // their new fingerprint instead of publishing stale scene bytes.
              const live = await readPreviewMetadata(fingerprintKey,
                () => api.diyChefFingerprint(sessionId, chefId, hatId), request.abort.signal)
              ensureNeeded()
              if (live !== scene.fingerprint) return false
              // Parallel sessions can prepare the same fingerprint. Only the
              // first captures; the remaining prepared scenes are disposed.
              if (retain(key)) return true
              let renderer: ReturnType<typeof chefRenderer> | undefined
              try {
                renderer = chefRenderer()
                rendererOwnsPrepared = true
                renderer.setChef(prepared!)
                const image = await renderer.capture()
                ensureNeeded()
                // Partial images only belong to their visible consumers and
                // cannot suppress a later retry with unchanged file metadata.
                if (scene.status === 'partial') key += `:partial:${++partialSequence}`
                putThumbnail(key, image, scene.status === 'partial' ? 'partial' : 'ready', scene.diagnostics)
                retain(key)
                if (scene.status === 'complete') completeImage = image
                return true
              } finally {
                if (renderer) renderer.clear()
                releaseChefRenderer()
              }
            }, request.abort.signal)
            if (!current) continue
            if (completeImage) {
              const bytes = Array.from(new Uint8Array(await completeImage.arrayBuffer()))
              ensureNeeded()
              await api.diyChefThumbnail(key, bytes, false).catch(() => undefined)
            }
            return key
          } finally {
            if (!rendererOwnsPrepared) prepared?.dispose()
            releaseMemory()
          }
        }
      }, request.abort.signal)
    }).finally(() => { if (requests.get(requestKey) === request) requests.delete(requestKey) })
  }
  pending.users.add(user)
  const request = pending
  const cancel = () => {
    request.users.delete(user)
    if (!request.users.size) request.abort.abort()
  }
  signal?.addEventListener('abort', cancel, { once: true })
  if (signal?.aborted) cancel()
  try {
    const key = await request.promise
    if (epoch !== cacheEpoch || request.abort.signal.aborted || signal?.aborted) throw new DOMException('Aborted', 'AbortError')
    const result = acquireThumbnail(key)
    if (!result) throw new Error('diychef.previewUnavailable')
    return result
  } catch (error) {
    return { url: '', state: 'error', diagnostics: [error instanceof Error ? error.message : String(error)], release() {} }
  } finally {
    cancel()
    signal?.removeEventListener('abort', cancel)
    if (!request.users.size) {
      request.thumbnail?.release()
      request.thumbnail = undefined
    }
  }
}
