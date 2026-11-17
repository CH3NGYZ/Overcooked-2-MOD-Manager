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
  let appliedAgain = false

  watch([dashboard.config, dashboard.mods, maps.maps, levels.levelSets,
    () => dashboard.game.value?.path, () => dashboard.game.value?.kitchen_running, demoActive], () => {
    changes.clearEvaluation()
    if (!appliedRefresh && !demoActive.value && isTauri() && dashboard.game.value?.path)
      void changes.invalidate().catch(() => undefined)
  }, { flush: 'post' })

  function refreshApplied() {
    appliedAgain = true
    if (appliedRefresh) return appliedRefresh
    changes.clearEvaluation()
    appliedRefresh = Promise.resolve().then(async () => {
      do {
        appliedAgain = false
        await Promise.all([dashboard.load(), maps.loadMaps({ force: true })])
        await levels.scanLevelSets()
        if (!disposed) await changes.invalidate()
      } while (appliedAgain && !disposed)
    }).finally(() => { appliedRefresh = undefined })
    return appliedRefresh
  }

  onMounted(() => {
    if (isTauri()) void watchModChangesApplied(() => { void refreshApplied().catch(() => undefined) })
      .then(unlisten => { if (disposed) unlisten(); else stop = unlisten })
      .catch(() => undefined)
  })
  onBeforeUnmount(() => { disposed = true; stop?.() })
}
