import { readonly, shallowReactive } from 'vue'
import type { AppDistribution, AppUpdateCheckResult, AppUpdateInfo, DownloadProgress } from './types.ts'

export type UpdateStage = 'idle' | 'required' | 'installing' | 'handoff' | 'manual'
  | 'backgroundRequired' | 'downloading' | 'stageFailed' | 'ready' | 'applying'

// 启动与手动检查共用一个门；路由卸载不能让已知新版被跳过。
const state = shallowReactive<{
  distribution: AppDistribution | null
  update: AppUpdateInfo | null
  stage: UpdateStage
  error: string
  progress: DownloadProgress | null
}>({ distribution: null, update: null, stage: 'idle', error: '', progress: null })

export const appUpdateState = readonly(state)

export function appUpdateBusy(): boolean {
  return state.stage === 'installing' || state.stage === 'handoff'
    || state.stage === 'downloading' || state.stage === 'applying'
}

export function setAppDistribution(distribution: AppDistribution): void {
  state.distribution = distribution
}

// 发行渠道由运行中的包身份决定；商店检查不能触发独立安装器的强制更新门。
export function applyAppUpdateCheck(result: AppUpdateCheckResult): void {
  setAppDistribution(result.distribution ?? state.distribution ?? 'standalone')
  if (state.distribution === 'microsoft_store' || result.all_failed) return
  if (result.update) requireAppUpdate(result.update)
}

export function requireAppUpdate(update: AppUpdateInfo): void {
  if (state.distribution === 'microsoft_store' || state.stage !== 'idle') return
  state.update = update
  state.stage = update.background_install && update.source_channel ? 'backgroundRequired' : 'required'
}

export function beginAppUpdate(): boolean {
  if (state.stage !== 'required') return false
  state.stage = 'installing'
  return true
}

export function finishAppUpdate(ok: boolean, error = ''): void {
  if (state.stage !== 'installing') return
  state.stage = ok ? 'handoff' : 'manual'
  state.error = ok ? '' : error
}

export function beginBackgroundAppUpdate(): string | undefined {
  if (state.stage !== 'backgroundRequired' && state.stage !== 'stageFailed') return
  const channel = state.update?.source_channel
  if (!channel) return
  state.error = ''
  state.progress = null
  state.stage = 'downloading'
  return channel
}

export function setBackgroundAppUpdateProgress(progress: DownloadProgress): void {
  if (state.stage === 'downloading') state.progress = progress
}

export function finishBackgroundAppUpdate(error = ''): void {
  if (state.stage !== 'downloading') return
  state.progress = null
  state.error = error
  if (!error) state.stage = 'ready'
  else state.stage = error === 'update.deferredUnsupported' ? 'manual' : 'stageFailed'
}

export function beginStagedAppUpdate(): boolean {
  if (state.stage !== 'ready') return false
  state.stage = 'applying'
  return true
}

export function finishStagedAppUpdate(error?: unknown): boolean {
  if (state.stage !== 'applying') return false
  if (error === 'update.stagedUnavailable' || error instanceof Error && error.message === 'update.stagedUnavailable') {
    state.error = 'update.stagedUnavailable'
    state.stage = 'stageFailed'
    return true
  }
  state.stage = error === undefined ? 'handoff' : 'ready'
  return false
}
