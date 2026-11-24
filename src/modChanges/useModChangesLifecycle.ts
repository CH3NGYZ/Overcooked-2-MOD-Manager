import { computed, onBeforeUnmount, onMounted, watch } from 'vue'
import { useDashboard } from '../composables/useDashboardStore'
import { useLevelSetScan } from '../composables/useLevelSetScan'
import { useMapSources } from '../composables/useMapSources'
import { watchLevelSetsChanged, watchLocalModsChanged } from '../services/tauri'
import { demoActive } from '../tutorial/demo'
import { isTauri } from '../runtime'
import { onlineRefreshPending } from '../refreshState'
import { useModChanges } from './store'

/** Owned by the App's only plan dialog, below the notification provider. */
export function useModChangesLifecycle() {
  const changes = useModChanges(), dashboard = useDashboard()
  const maps = useMapSources(), levels = useLevelSetScan()
  const executing = computed(() => changes.state.value?.phase === 'executing')
  const gameKnown = computed(() => dashboard.game.value != null)
  const path = computed(() => dashboard.game.value?.path)
  const stops = new Set<() => void>()
  let disposed = false
  let committed = false
  let refreshing = false
  let queued: 'mods' | 'levels' | undefined
  let onlineDirty = false

  // An established "no game found" also needs a scan to settle the empty
  // map list and to let Rust release the previous root's watcher.
  const canRead = () => !disposed && !demoActive.value && isTauri() && gameKnown.value
  const awaitingCommit = () => executing.value && !committed

  function evaluate() {
    if (!path.value || !levels.hasCurrentResult.value) return
    if (!onlineRefreshPending.value) onlineDirty = false
    void changes.invalidate().catch(() => undefined)
  }

  function startRefresh() {
    changes.setEvaluationPaused(refreshing || !!queued || awaitingCommit())
    if (refreshing || !queued || !canRead() || awaitingCommit()) return
    refreshing = true
    void Promise.resolve().then(async () => {
      let ready = false
      try {
        while (queued && canRead() && !awaitingCommit()) {
          const scope = queued
          queued = undefined
          ready = false
          if (scope === 'mods') await dashboard.loadLocalMods({ afterPending: true })
          if (!canRead() || awaitingCommit()) break
          if (queued === 'mods') continue
          levels.setWaitingForLocalMods(false)
          if (dashboard.modsStatus.value === 'error') { queued = undefined; break }
          // Map edits before this read are included here. If an edit interrupts
          // a scan, the scanner rejects the overtaken result before publishing.
          queued = undefined
          try {
            await levels.scanLevelSets({ reuseCurrentGeneration: true })
            ready = levels.hasCurrentResult.value
          } catch { /* Retain the visible snapshot; the scanner owns the error. */ }
        }
      } finally {
        refreshing = false
        if (!disposed) {
          if (queued !== 'mods') levels.setWaitingForLocalMods(false)
          changes.setEvaluationPaused(!!queued || awaitingCommit())
        }
      }
      if (ready && !queued && canRead() && !awaitingCommit()) evaluate()
    }).catch(() => undefined)
  }

  function requestRefresh(scope: 'mods' | 'levels') {
    if (disposed) return
    if (scope === 'mods' || !queued) queued = scope
    if (scope === 'mods') levels.setWaitingForLocalMods(true)
    startRefresh()
  }

  // In a different context the old actions are not meaningful. During reads
  // in the same context keep the last complete conclusion until replacement.
  watch([levels.contextRevision, demoActive], changes.clearEvaluation, { flush: 'sync' })
  watch(path, changes.invalidateRecoveryApproval, { flush: 'sync' })
  watch(changes.appliedRevision, () => {
    committed = true
    requestRefresh('mods')
  }, { flush: 'sync' })
  watch(executing, active => {
    if (active) committed = false
    // A failed write can revoke the backend generation even if rollback is
    // clean and no successful-commit event is emitted.
    if (!active && !committed && changes.state.value?.phase === 'failed') requestRefresh('mods')
    else startRefresh()
    if (!active && !refreshing && !queued && canRead() && onlineDirty && !onlineRefreshPending.value) evaluate()
  }, { flush: 'sync' })

  watch([dashboard.config, maps.maps, levels.levelSets, dashboard.modsGeneration, levels.contextRevision,
    path, gameKnown, () => dashboard.game.value?.kitchen_running, demoActive, onlineRefreshPending],
  ([config, catalog, sets, generation, context, gamePath, known, running, demo, onlineBusy],
    [oldConfig, oldCatalog, oldSets, oldGeneration, oldContext, oldPath, oldKnown, oldRunning, oldDemo]) => {
    onlineDirty ||= catalog !== oldCatalog || config?.repositories !== oldConfig?.repositories
      || config?.repository_sources !== oldConfig?.repository_sources || config?.data_provider !== oldConfig?.data_provider
      || config?.region !== oldConfig?.region || config?.platform !== oldConfig?.platform
    if (!canRead()) return
    if (queued) { startRefresh(); return }
    if (refreshing) return
    // Finish the inventory on initial load, manual DLL reads, or a context
    // change. Reuse a scan if it already covers the current DLL generation.
    if (context !== oldContext || gamePath !== oldPath || known !== oldKnown || generation !== oldGeneration || demo !== oldDemo) {
      requestRefresh('levels')
      return
    }
    if (!awaitingCommit() && (sets !== oldSets || running !== oldRunning || (!onlineBusy && onlineDirty))) evaluate()
  }, { flush: 'post' })

  onMounted(() => {
    if (!isTauri()) return
    const retain = (subscription: Promise<() => void>) => {
      void subscription.then(unlisten => { if (disposed) unlisten(); else stops.add(unlisten) }).catch(() => undefined)
    }
    retain(watchLocalModsChanged(() => requestRefresh('mods')))
    retain(watchLevelSetsChanged(() => {
      if (disposed) return
      levels.invalidate()
      requestRefresh('levels')
    }))
    if (canRead() && !levels.hasCurrentResult.value) requestRefresh('levels')
  })
  onBeforeUnmount(() => {
    disposed = true
    queued = undefined
    levels.setWaitingForLocalMods(false)
    changes.setEvaluationPaused(false)
    for (const stop of stops) stop()
    stops.clear()
  })
}
