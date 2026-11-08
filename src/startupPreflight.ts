import type { StartupProbe } from './types.ts'

export type PreflightStage = 'idle' | 'region' | 'checking' | 'failed' | 'closing' | 'completed'

interface Dependencies {
  desktop: boolean
  region: () => Promise<string>
  probe: () => Promise<StartupProbe>
  cancel: () => Promise<void>
  beforeProbe: () => Promise<void>
  ready: () => void
  changed: (stage: PreflightStage, failure: string) => void
}

/** One app lifetime owns the gate; pages and tutorial callbacks only request it. */
export function createStartupPreflight(deps: Dependencies) {
  let stage: PreflightStage = 'idle'
  let active: Promise<void> | undefined
  let closing = false
  const change = (next: PreflightStage, failure = '') => {
    stage = next
    deps.changed(next, failure)
  }
  const finish = () => {
    if (stage === 'completed') return
    change('completed')
    deps.ready()
  }
  const check = async () => {
    change('checking')
    await deps.beforeProbe()
    if (closing) return
    let result: StartupProbe
    try {
      result = await deps.probe()
    } catch {
      result = { reachable: false, failure_kind: 'network' }
    }
    if (closing) return
    if (result.reachable) finish()
    else change('failed', result.failure_kind ?? 'network')
  }
  return {
    start(): Promise<void> {
      if (stage !== 'idle') return active ?? Promise.resolve()
      if (!deps.desktop) { finish(); return Promise.resolve() }
      change('region')
      active = (async () => {
        try {
          const region = await deps.region()
          if (closing) return
          if (region === 'chinese_mainland') { finish(); return }
        } catch {
          if (closing) return
          change('failed', 'region')
          return
        }
        await check()
      })()
      return active
    },
    retry(): Promise<void> {
      if (stage !== 'failed' || closing) return active ?? Promise.resolve()
      // Retry includes a fresh region read, since the user may have changed it.
      change('idle')
      return this.start()
    },
    async close(): Promise<void> {
      if (stage === 'completed' || closing) return
      closing = true
      change('closing')
      try { await deps.cancel() } catch { /* settle even if IPC cancellation fails */ }
      await active
      finish()
    },
  }
}
