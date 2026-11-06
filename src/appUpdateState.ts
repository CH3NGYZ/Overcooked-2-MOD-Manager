import { readonly, shallowReactive } from 'vue'
import type { AppUpdateInfo } from './types.ts'

type UpdateStage = 'idle' | 'required' | 'installing' | 'handoff' | 'manual'

// 启动与手动检查共用一个门；路由卸载不能让已知新版被跳过。
const state = shallowReactive<{
  update: AppUpdateInfo | null
  stage: UpdateStage
  error: string
}>({ update: null, stage: 'idle', error: '' })

export const appUpdateState = readonly(state)

export function requireAppUpdate(update: AppUpdateInfo): void {
  if (state.stage !== 'idle') return
  state.update = update
  state.stage = 'required'
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
