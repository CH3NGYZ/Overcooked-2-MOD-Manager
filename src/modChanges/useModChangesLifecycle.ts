import { onBeforeUnmount, onMounted, watch } from 'vue'
import { useDashboard } from '../composables/useDashboardStore'
import { useLevelSetScan } from '../composables/useLevelSetScan'
import { useMapSources } from '../composables/useMapSources'
import { watchLocalModsChanged, watchModChangesApplied } from '../services/tauri'
import { demoActive } from '../tutorial/demo'
import { isTauri } from '../runtime'
import { onlineRefreshPending } from '../refreshState'
import { useModChanges } from './store'

/** Owned by the App's only plan dialog, below the notification provider. */
export function useModChangesLifecycle() {
  const changes = useModChanges(), dashboard = useDashboard()
  const maps = useMapSources(), levels = useLevelSetScan()
  let disposed = false
  const stops = new Set<() => void>()
  let appliedRefresh: Promise<void> | undefined
  let pendingRefresh: { scope: 'local' | 'full', mods: Promise<void> } | undefined
  let evaluatingApplied = false
  let externalChangeDuringDemo = false
  let onlineDirty = false

  watch([dashboard.config, maps.maps, levels.levelSets, dashboard.modsGeneration, dashboard.diyLevelTarget,
    () => dashboard.game.value?.path, () => dashboard.game.value?.kitchen_running, demoActive, onlineRefreshPending],
  ([config, catalog, sets, generation, target, path, running, demo, onlineBusy],
    [oldConfig, oldCatalog, oldSets, oldGeneration, oldTarget, oldPath, oldRunning, oldDemo]) => {
    onlineDirty ||= catalog !== oldCatalog || config?.repositories !== oldConfig?.repositories
      || config?.repository_sources !== oldConfig?.repository_sources || config?.data_provider !== oldConfig?.data_provider
      || config?.region !== oldConfig?.region || config?.platform !== oldConfig?.platform
    if (externalChangeDuringDemo && !demoActive.value) {
      externalChangeDuringDemo = false
      void refreshApplied('local').catch(() => undefined)
    }
    if (appliedRefresh && (!evaluatingApplied || pendingRefresh)) return
    // A scan generation represents disk changes. A rebinding pass only
    // projects the same DLL facts and must not re-evaluate the inventory.
    const localChanged = sets !== oldSets || generation !== oldGeneration || target !== oldTarget
      || path !== oldPath || running !== oldRunning || demo !== oldDemo
      || config?.game?.path !== oldConfig?.game?.path || config?.game?.runtime !== oldConfig?.game?.runtime
    if (localChanged) changes.clearEvaluation()
    if (demo || !isTauri() || !path || (!localChanged && (onlineBusy || !onlineDirty))) return
    // Keep the last relation snapshot and its actions live as individual
    // online results arrive. Explicit plan requests still read current facts.
    if (!onlineBusy) onlineDirty = false
    void changes.invalidate().catch(() => undefined)
  }, { flush: 'post' })

  function refreshApplied(scope: 'local' | 'full') {
    if (!pendingRefresh) {
      // Mark the current DLL generation unusable before an older map response
      // can publish. The actual reads still coalesce against the fresh snapshot.
      pendingRefresh = {
        scope,
        mods: dashboard.loadLocalMods({ afterPending: true }),
      }
      changes.clearEvaluation()
    } else if (scope === 'full') pendingRefresh.scope = 'full'
    if (appliedRefresh) return appliedRefresh
    appliedRefresh = Promise.resolve().then(async () => {
      while (pendingRefresh && !disposed) {
        const { scope, mods } = pendingRefresh
        pendingRefresh = undefined
        if (scope === 'local') await mods
        else await Promise.all([mods, dashboard.loadBase(), maps.loadMaps({ force: true })])
        if (pendingRefresh || disposed) continue
        await levels.scanLevelSets({ reuseCurrentGeneration: true }).catch(() => undefined)
        if (!pendingRefresh && !disposed) {
          if (!onlineRefreshPending.value) onlineDirty = false
          evaluatingApplied = true
          try { await changes.invalidate().catch(() => undefined) }
          finally { evaluatingApplied = false }
        }
      }
    }).finally(() => {
      appliedRefresh = undefined
      if (pendingRefresh && !disposed) void refreshApplied(pendingRefresh.scope).catch(() => undefined)
    })
    return appliedRefresh
  }

  onMounted(() => {
    if (!isTauri()) return
    const retain = (subscription: Promise<() => void>) => {
      void subscription.then(unlisten => { if (disposed) unlisten(); else stops.add(unlisten) }).catch(() => undefined)
    }
    retain(watchModChangesApplied(event => {
      if (!disposed) void refreshApplied(event?.map_toggle_only === true ? 'local' : 'full').catch(() => undefined)
    }))
    retain(watchLocalModsChanged(() => {
      if (disposed) return
      if (demoActive.value) externalChangeDuringDemo = true
      else void refreshApplied('local').catch(() => undefined)
    }))
  })
  onBeforeUnmount(() => {
    disposed = true
    pendingRefresh = undefined
    for (const stop of stops) stop()
    stops.clear()
  })
}
