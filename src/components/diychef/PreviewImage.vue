<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, shallowRef, watch } from 'vue'
import { PersonOutline } from '@vicons/ionicons5'
import { t } from '../../i18n'
import { chefThumbnailIdentity, requestChefThumbnail } from '../../composables/useDiyChefThumbnails'
import { acquireThumbnail, presentThumbnail } from '../../diychef/preview/cache'
import { previewWorkPaused } from '../../diychef/preview/queue'
import { diyChefDemoImage, isDiyChefDemoSession } from '../../tutorial/demoDiyChef'

const props = defineProps<{ sessionId: string; chefId: string; hatId: string | null; name: string }>()
const host = shallowRef<HTMLElement | null>(null)
// Cross-grid moves recreate this component. Reacquire the already decoded PNG
// before its first render, including while drag work remains paused.
const result = shallowRef<Awaited<ReturnType<typeof requestChefThumbnail>> | null>(
  props.hatId ? acquireThumbnail(chefThumbnailIdentity(props.sessionId, props.chefId, props.hatId)) : null)
const loading = shallowRef(false)
const failed = shallowRef(false)
const demoImage = computed(() => diyChefDemoImage(props.sessionId, props.chefId, props.hatId))
const imageUrl = computed(() => demoImage.value ?? result.value?.url)
let visible = false
let generation = 0
let observer: IntersectionObserver | null = null
let abort: AbortController | null = null
let settling: ReturnType<typeof setTimeout> | undefined
let lastKey: string | undefined

function release() {
  if (result.value?.state === 'ready') lastKey = result.value.key
  result.value?.release()
  result.value = null
}
function stop() {
  if (settling) clearTimeout(settling)
  settling = undefined
  generation += 1
  abort?.abort()
  abort = null
  loading.value = false
}
async function decode(url: string, signal: AbortSignal) {
  const image = new Image()
  await new Promise<void>((resolve, reject) => {
    let finished = false
    const finish = (error?: unknown) => {
      if (finished) return
      finished = true
      image.onload = image.onerror = null
      signal.removeEventListener('abort', cancel)
      if (error) reject(error)
      else resolve()
    }
    const cancel = () => { finish(new DOMException('Aborted', 'AbortError')); image.src = '' }
    // A load event can precede decode. Only browsers without decode use it as
    // the ready boundary; otherwise it must not bypass the pending pixels.
    image.onload = typeof image.decode === 'function' ? null : () => finish()
    image.onerror = () => finish(new Error('diychef.previewUnavailable'))
    signal.addEventListener('abort', cancel, { once: true })
    image.src = url
    // decode resolves only when the pixels are presentable, unlike obtaining a
    // Blob URL. Keep the old lease until Vue has installed this decoded URL.
    if (typeof image.decode === 'function') void image.decode().then(() => finish(), finish)
    if (signal.aborted) cancel()
  })
}
async function load() {
  // Gate every path, including warm PNG reads and their browser decode. The
  // visible image keeps its existing lease while dragging settles.
  if (previewWorkPaused.value) return
  const ticket = ++generation
  abort?.abort()
  abort = null
  if (isDiyChefDemoSession(props.sessionId)) { loading.value = false; failed.value = !demoImage.value; release(); return }
  if (!visible || !props.hatId) { loading.value = false; failed.value = false; release(); return }
  abort = new AbortController()
  const control = abort
  loading.value = true
  failed.value = false
  let pending: Awaited<ReturnType<typeof requestChefThumbnail>> | null = null
  try {
    pending = await requestChefThumbnail(props.sessionId, props.chefId, props.hatId, control.signal)
    if (ticket !== generation || !visible) return
    if (pending.state === 'error' || !pending.url) throw new Error('diychef.previewUnavailable')
    if (pending.url !== imageUrl.value) await decode(pending.url, control.signal)
    if (ticket !== generation || !visible || control.signal.aborted) return
    const previous = result.value
    presentThumbnail(chefThumbnailIdentity(props.sessionId, props.chefId, props.hatId), pending.key)
    result.value = pending
    pending = null
    await nextTick()
    previous?.release()
  } catch { if (ticket === generation) failed.value = !imageUrl.value }
  finally { pending?.release(); if (ticket === generation) loading.value = false }
}
function settle() {
  if (!visible || previewWorkPaused.value) return
  settling = setTimeout(() => { settling = undefined; void load() }, 90)
}
watch(previewWorkPaused, paused => {
  // Abort synchronously so an already-running invoke cannot publish or start
  // follow-up decoding before the next Vue update flush.
  stop()
  if (!paused) settle()
}, { flush: 'sync' })
watch(() => [props.sessionId, props.chefId, props.hatId], (next, before) => {
  stop()
  // Hat/session changes replace the same chef's picture in place. A different
  // chef must never inherit that image or its remembered warm-cache key.
  if (next[1] !== before[1]) { release(); lastKey = undefined }
  void load()
})
onMounted(() => {
  // A viewport root is still clipped by the nested scrollbar, so its margin
  // cannot preload rows outside that scrollport. Expand the scrollport itself.
  const root = host.value?.closest<HTMLElement>('.n-scrollbar-container') ?? null
  observer = new IntersectionObserver(([entry]) => {
    const next = entry?.isIntersecting ?? false
    if (visible === next) { if (!next) release(); return }
    visible = next
    stop()
    if (visible) {
      // Reuse the existing image immediately, then validate live metadata once
      // this card settles. Passing cards never enqueue expensive native work.
      if (!result.value && lastKey) result.value = acquireThumbnail(lastKey)
      settle()
    } else release()
  // Keep two complete card rows above and below the scrollport. Cards beyond
  // this buffer release their leases; the bounded PNG LRU can still reuse them.
  }, { root, rootMargin: '480px 80px' })
  if (host.value) observer.observe(host.value)
})
onBeforeUnmount(() => { stop(); observer?.disconnect(); release() })
</script>

<template>
  <span ref="host" class="chef-portrait" :aria-busy="loading">
    <img v-if="imageUrl" :src="imageUrl" :alt="name" draggable="false" />
    <span v-else class="chef-portrait-fallback" aria-hidden="true"><PersonOutline /></span>
    <span v-if="loading && !imageUrl" class="chef-portrait-status loading" aria-hidden="true">···</span>
    <span v-else-if="failed || !hatId" class="chef-portrait-status">{{ t('diychef.previewUnavailable') }}</span>
    <span v-else-if="result?.state === 'partial'" class="chef-portrait-status">{{ t('diychef.previewPartial') }}</span>
  </span>
</template>

<style scoped>
.chef-portrait { position: relative; display: grid; place-items: center; width: 120px; max-width: 100%; aspect-ratio: 4 / 5; flex: 0 0 auto; }
.chef-portrait img { width: 100%; height: 100%; object-fit: contain; user-select: none; }
.chef-portrait-fallback { width: 55%; height: 55%; color: var(--oc2-text-4); }
.chef-portrait-status { position: absolute; bottom: 1px; max-width: 100%; padding: 0 5px; border-radius: 4px; background: var(--oc2-row); color: var(--oc2-text-3); font-size: 10px; line-height: 16px; }
.chef-portrait-status.loading { font-size: 18px; line-height: 15px; letter-spacing: 3px; }
</style>
