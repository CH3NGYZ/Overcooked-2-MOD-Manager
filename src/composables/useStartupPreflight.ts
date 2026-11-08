import { nextTick, shallowReactive } from 'vue'
import { api } from '../services/tauri'
import { isTauri } from '../runtime'
import { createStartupPreflight, type PreflightStage } from '../startupPreflight'

export const startupPreflightState = shallowReactive<{ stage: PreflightStage; failure: string }>({
  stage: 'idle', failure: '',
})

let gate: ReturnType<typeof createStartupPreflight> | undefined

export function useStartupPreflight(ready: () => void) {
  gate ??= createStartupPreflight({
    desktop: isTauri(),
    region: () => api.refreshUserRegion(),
    probe: () => api.probeStartupSource(),
    cancel: () => api.cancelStartupProbe(),
    beforeProbe: nextTick,
    ready,
    changed(stage, failure) { Object.assign(startupPreflightState, { stage, failure }) },
  })
  return gate
}
