import { readonly, shallowRef } from 'vue'
import { api } from '../services/tauri'
import { setRefreshSchedulingPaused } from '../refreshState'
import type { DataProvider, PublicConfig, UserRegion } from '../types'
import { demoActive } from '../tutorial/demo'
import { useDashboard } from './useDashboardStore'
import { useMapSources } from './useMapSources'
import { useModRepositoryRefresh } from './useModRepositoryRefresh'

// The operation survives leaving Settings; another instance cannot start a
// conflicting selection while the package or its source trees are loading.
const switching = shallowRef(false)

export type DataSelectionPatch = { region: UserRegion; data_provider?: never } | { data_provider: DataProvider; region?: never }

export function useDataProviderSwitch() {
  const dashboard = useDashboard()
  const mods = useModRepositoryRefresh()
  const maps = useMapSources()

  async function switchSelection(patch: DataSelectionPatch): Promise<{ config: PublicConfig; ready: boolean; errors: string[] } | null> {
    if (switching.value || demoActive.value) return null
    switching.value = true
    setRefreshSchedulingPaused(true)
    mods.invalidateRefresh()
    maps.invalidateRefresh()
    let schedulingResumed = false
    try {
      // The backend persists the region and chooses its initial provider as one
      // operation. Manual provider selection keeps its session-only lifetime.
      const selected = await api.saveSettings(patch)
      dashboard.applyConfigSnapshot(selected)
      let config: PublicConfig
      try {
        // This is the shared package publication gate, including network plans
        // and proxies. Source refresh below must reuse it, never force a ZIP.
        config = await api.refreshNetworkConfig()
      } catch {
        await dashboard.reloadConfig()
        return { config: dashboard.config.value ?? selected, ready: false, errors: [] }
      }
      // A busy refresh used to silently drop the new source selection. Drain
      // invalidated callbacks before starting the normal incremental pipelines.
      await Promise.all([mods.whenIdle(), maps.whenIdle()])
      dashboard.applyConfigSnapshot(config)
      setRefreshSchedulingPaused(false)
      schedulingResumed = true
      const [errors] = await Promise.all([
        mods.refreshEverything({ force: false }),
        maps.refreshAll({ force: false }),
      ])
      return { config: dashboard.config.value ?? config, ready: true, errors }
    } finally {
      // Failed selection/package preparation still owns the pause until old
      // callbacks clear their flags; a retry must not share those old slots.
      if (!schedulingResumed) await Promise.all([mods.whenIdle(), maps.whenIdle()])
      setRefreshSchedulingPaused(false)
      switching.value = false
    }
  }

  return { switching: readonly(switching), switchSelection }
}
