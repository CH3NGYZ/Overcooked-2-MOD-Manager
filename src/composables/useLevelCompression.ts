import { shallowRef } from 'vue'
import { api, watchLevelSetCompression } from '../services/tauri'
import type { LevelSetCompressionProgress } from '../types'

const compression = shallowRef<LevelSetCompressionProgress | null>(null)
let listening: Promise<unknown> | undefined
let revision = 0

/** Global titlebar subscribes once; late IPC snapshots cannot overwrite newer queue events. */
export function useLevelCompression() {
  listening ??= (async () => {
    await watchLevelSetCompression(next => { revision++; compression.value = next })
    const before = revision
    const snapshot = await api.levelCompressionProgress()
    if (before === revision) compression.value = snapshot
  })().catch(() => { listening = undefined })
  return { compression }
}
