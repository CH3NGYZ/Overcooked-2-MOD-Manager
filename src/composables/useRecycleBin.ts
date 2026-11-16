import { computed, shallowRef } from 'vue'
import { api, watchRecycleBin } from '../services/tauri'
import { backendMessageOf } from '../locale/backend'
import { isTauri } from '../runtime'
import type { ModRecycleEntry } from '../types'

// The title-bar menu and full dialog share the list, totals and operation gate.
const entries = shallowRef<ModRecycleEntry[]>([])
const loading = shallowRef(false)
const loaded = shallowRef(false)
const error = shallowRef('')
const acting = shallowRef<{ kind: 'restore' | 'delete' | 'clear'; id?: string } | null>(null)
const busy = computed(() => acting.value !== null)
const count = computed(() => entries.value.length)
const totalBytes = computed(() => entries.value.reduce((total, entry) => total + entry.size_bytes, 0))
const overLimit = computed(() => totalBytes.value > 1024 ** 3)
let listening: Promise<unknown> | undefined
let pending: Promise<void> | undefined
let revision = 0

function ensureListener() {
  // Subscribe before taking a snapshot so a concurrent uninstall cannot be lost.
  listening ??= watchRecycleBin(() => { void reload() }).catch(() => { listening = undefined })
  return listening
}

function reload(): Promise<void> {
  if (!isTauri()) return Promise.resolve()
  revision++
  if (pending) return pending
  loading.value = true
  pending = (async () => {
    await ensureListener()
    for (;;) {
      const before = revision
      try {
        const snapshot = await api.getModRecycleBin()
        if (before !== revision) continue
        entries.value = snapshot
        loaded.value = true
        error.value = ''
      } catch (reason) {
        if (before !== revision) continue
        // Retain the last trustworthy list and size if a refresh fails.
        error.value = backendMessageOf(reason)
      }
      break
    }
  })().finally(() => { pending = undefined; loading.value = false })
  return pending
}

function ensureLoaded(): Promise<void> {
  return pending ?? (loaded.value ? Promise.resolve() : reload())
}

export function useRecycleBin() {
  return { entries, loading, loaded, error, count, totalBytes, overLimit, busy, acting, reload, ensureLoaded }
}
