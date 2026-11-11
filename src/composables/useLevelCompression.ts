import { computed, shallowRef, watch } from 'vue'
import { api, watchLevelSetCompression } from '../services/tauri'
import type { LevelSetCompressionProgress } from '../types'
import { demoActive, withDemoLevelCompression } from '../tutorial/demo'

const realCompression = shallowRef<LevelSetCompressionProgress | null>(null)
const compression = computed(() => withDemoLevelCompression(realCompression.value))
let initialized = false
let subscribed = false
let initialization: Promise<void> | undefined
let revision = 0

function ensureCompressionStatus(): Promise<void> | undefined {
  if (demoActive.value || initialized) return
  if (initialization) return initialization
  initialization = (async () => {
    if (!subscribed) {
      await watchLevelSetCompression(next => { revision++; realCompression.value = next })
      subscribed = true
    }
    // 教程可能在事件订阅就绪前开始，真实快照留到退出后再读。
    if (demoActive.value) return
    const before = revision
    const snapshot = await api.levelCompressionProgress()
    if (before === revision) realCompression.value = snapshot
    initialized = true
  })().catch(() => undefined).finally(() => { initialization = undefined })
  return initialization
}

watch(demoActive, active => {
  if (!active) void ensureCompressionStatus()
}, { flush: 'sync' })

/** Global titlebar subscribes once; late IPC snapshots cannot overwrite newer queue events. */
export function useLevelCompression() {
  void ensureCompressionStatus()
  return { compression }
}
