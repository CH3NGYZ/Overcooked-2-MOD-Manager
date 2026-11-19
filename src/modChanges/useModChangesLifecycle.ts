import { onBeforeUnmount, onMounted, watch } from 'vue'
import { useDashboard } from '../composables/useDashboardStore'
import { useLevelSetScan } from '../composables/useLevelSetScan'
import { useMapSources } from '../composables/useMapSources'
import { watchModChangesApplied } from '../services/tauri'
import { demoActive } from '../tutorial/demo'
import { isTauri } from '../runtime'
import { useModChanges } from './store'

/** Owned by the App's only plan dialog, below the notification provider. */
export function useModChangesLifecycle() {
  const changes = useModChanges(), dashboard = useDashboard()
  const maps = useMapSources(), levels = useLevelSetScan()
  let disposed = false
  let stop: (() => void) | undefined
  let appliedRefresh: Promise<void> | undefined
  let pendingRefresh: { scope: 'local' | 'full', mods: Promise<void> } | undefined
  let evaluatingApplied = false

  watch([dashboard.config, dashboard.mods, maps.maps, levels.levelSets,
    () => dashboard.game.value?.path, () => dashboard.game.value?.kitchen_running, demoActive], () => {
    if (appliedRefresh && (!evaluatingApplied || pendingRefresh)) return
    changes.clearEvaluation()
    if (!demoActive.value && isTauri() && dashboard.game.value?.path)
      void changes.invalidate().catch(() => undefined)
  }, { flush: 'post' })

  function refreshApplied(mapToggleOnly: boolean) {
    if (!pendingRefresh) {
      // Mark the current DLL generation unusable before an older map response
      // can publish. The actual reads still coalesce against the fresh snapshot.
      pendingRefresh = {
        scope: mapToggleOnly ? 'local' : 'full',
        mods: dashboard.loadLocalMods({ afterPending: true }),
      }
      changes.clearEvaluation()
    } else if (!mapToggleOnly) pendingRefresh.scope = 'full'
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
          evaluatingApplied = true
          try { await changes.invalidate().catch(() => undefined) }
          finally { evaluatingApplied = false }
        }
      }
    }).finally(() => { appliedRefresh = undefined })
    return appliedRefresh
  }

  onMounted(() => {
    if (isTauri()) void watchModChangesApplied(event => { void refreshApplied(event?.map_toggle_only === true).catch(() => undefined) })
      .then(unlisten => { if (disposed) unlisten(); else stop = unlisten })
      .catch(() => undefined)
  })
  onBeforeUnmount(() => { disposed = true; stop?.() })
}
