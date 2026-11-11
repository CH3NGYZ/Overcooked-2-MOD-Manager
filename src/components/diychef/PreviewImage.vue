<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, shallowRef, watch } from 'vue'
import { PersonOutline } from '@vicons/ionicons5'
import { t } from '../../i18n'
import { requestChefThumbnail } from '../../composables/useDiyChefThumbnails'
import { acquireThumbnail } from '../../diychef/preview/cache'
import { diyChefDemoImage, isDiyChefDemoSession } from '../../tutorial/demoDiyChef'

const props = defineProps<{ sessionId: string; chefId: string; hatId: string | null; name: string }>()
const host = shallowRef<HTMLElement | null>(null)
const result = shallowRef<Awaited<ReturnType<typeof requestChefThumbnail>> | null>(null)
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
async function load() {
  const ticket = ++generation
  abort?.abort()
  abort = null
  if (isDiyChefDemoSession(props.sessionId)) { loading.value = false; failed.value = !demoImage.value; release(); return }
  if (!visible || !props.hatId) { loading.value = false; failed.value = false; release(); return }
  abort = new AbortController()
  loading.value = true
  failed.value = false
  try {
    const next = await requestChefThumbnail(props.sessionId, props.chefId, props.hatId, abort.signal)
    if (ticket !== generation || !visible) { next.release(); return }
    release()
    result.value = next
    failed.value = next.state === 'error'
  } catch { if (ticket === generation) failed.value = true }
  finally { if (ticket === generation) loading.value = false }
}
watch(() => [props.sessionId, props.chefId, props.hatId], (next, before) => {
  stop()
  if (next[1] !== before[1] || next[2] !== before[2]) { release(); lastKey = undefined }
  void load()
})
onMounted(() => {
  // A viewport root is still clipped by the nested scrollbar, so its margin
  // cannot preload rows outside that scrollport. Expand the scrollport itself.
  const root = host.value?.closest<HTMLElement>('.n-scrollbar-container') ?? null
  observer = new IntersectionObserver(([entry]) => {
    const next = entry?.isIntersecting ?? false
    if (visible === next) return
    visible = next
    stop()
    if (visible) {
      // Reuse the existing image immediately, then validate live metadata once
      // this card settles. Passing cards never enqueue expensive native work.
      if (!result.value && lastKey) result.value = acquireThumbnail(lastKey)
      settling = setTimeout(() => { settling = undefined; void load() }, 90)
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
