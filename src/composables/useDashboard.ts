import type { Ref } from 'vue'
import type { Dashboard, GameRuntime, WinhttpState } from '../types'
import { api } from '../services/tauri'
import { t } from '../i18n'

/**
 * Serialized access to the dashboard/config snapshots. Every full read goes
 * through one queue so an older snapshot can never overwrite a newer one while
 * repository refreshes and game events arrive concurrently.
 */
export function useDashboard(options: {
  dashboard: Ref<Dashboard | null>
  run: <T>(label: string, action: () => Promise<T>) => Promise<T>
  requestConfirmation: (request: { title: string; message: string; danger?: boolean; acknowledge?: boolean }) => Promise<boolean>
  applyStartupZoom: (value: number | undefined) => Promise<void>
  isBlocked: () => boolean
  onRuntimeChanged: () => void
  currentWinhttp: () => WinhttpState
  api: typeof api
}) {
  const { dashboard, run, requestConfirmation, applyStartupZoom, isBlocked, onRuntimeChanged, currentWinhttp, api } = options
  let updateQueue: Promise<void> = Promise.resolve()

  function getUpdateQueue() { return updateQueue }

  async function refreshDashboardSerialized() {
    const next = updateQueue.then(async () => {
      dashboard.value = await api.dashboard()
    })
    updateQueue = next.catch(() => undefined)
    await next
    return dashboard.value
  }

  async function refreshConfigSerialized() {
    const next = updateQueue.then(async () => {
      const savedConfig = await api.config()
      if (dashboard.value) dashboard.value = { ...dashboard.value, config: savedConfig }
    })
    updateQueue = next.catch(() => undefined)
    await next
  }

  async function loadDashboard() {
    await run('正在读取厨房状态', async () => {
      dashboard.value = await api.dashboard()
      await applyStartupZoom(dashboard.value.config.ui_zoom)
    })
  }

  async function detectGame() { await run('重新检测游戏目录', async () => { await api.detectGame(); dashboard.value = await api.dashboard() }) }
  async function chooseGameDirectory() { const path = await api.chooseGameDirectory(); if (!path || Array.isArray(path)) return; await run('正在保存厨房目录', async () => { await api.setGamePath(path); dashboard.value = await api.dashboard() }) }
  async function restoreDefaultGameDirectory() { await run('正在恢复默认游戏目录', async () => { await api.resetGamePath(); dashboard.value = await api.dashboard() }) }

  async function changeGameRuntime(runtime: GameRuntime) {
    if (isBlocked()) return
    if (dashboard.value?.config.game.runtime === runtime) return
    await run('正在切换厨房运行模式', async () => {
      await api.saveSettings({ game_runtime: runtime })
      onRuntimeChanged()
      await loadDashboard()
    })
  }

  async function launchNormal() { await run('正在准备 Steam 启动', api.launchNormal); await loadDashboard() }

  async function toggleWinhttp() {
    if (currentWinhttp() === 'missing') return
    const enabled = currentWinhttp() === 'moddisabled' || currentWinhttp() === 'conflict'
    await run('正在切换框架状态', () => api.switchWinhttp(enabled))
    await loadDashboard()
  }

  async function openDirectory() { await run('正在打开游戏目录', api.openGameDirectory) }
  async function openModDirectory() { await run('正在打开模组目录', api.openModDirectory) }

  async function resetUi() {
    if (!await requestConfirmation({ title: t('重置 UI'), message: `${t('重置 UI')}？` })) return
    await run('正在重置界面', api.resetUi)
    await loadDashboard()
  }

  return {
    getUpdateQueue, refreshDashboardSerialized, refreshConfigSerialized, loadDashboard,
    detectGame, chooseGameDirectory, restoreDefaultGameDirectory, changeGameRuntime,
    launchNormal, toggleWinhttp, openDirectory, openModDirectory, resetUi,
  }
}
