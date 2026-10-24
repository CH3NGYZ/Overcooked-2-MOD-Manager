import { computed, ref, type Ref } from 'vue'
import type { AppUpdateChannelEvent, AppUpdateCheckResult, AppUpdateInfo, AppUpdatePhaseEvent } from '../types'
import type { Locale } from '../i18n'
import { t } from '../i18n'
import { api } from '../services/tauri'

/**
 * Startup installer-update state machine: region/channel manifest events,
 * download phases, the skip menu with its manual-update countdown, and the
 * in-app re-check / install entry used by the overview update button.
 */
export function useAppUpdate(options: {
  run: <T>(label: string, action: () => Promise<T>) => Promise<T>
  locale: Ref<Locale>
  startupStage: Ref<string>
  startupError: Ref<string>
  showSuccess: (value: string) => void
  isUnmounted: () => boolean
  beginDownload: () => void
  clearDownload: () => void
}) {
  const { run, locale, startupStage, startupError, showSuccess, isUnmounted, beginDownload, clearDownload } = options
  const appUpdate = ref<AppUpdateInfo | null>(null)
  const appUpdateCheck = ref<AppUpdateCheckResult | null>(null)
  const startupUpdateChecking = ref(true)
  const updateCheckFailed = ref(false)
  const updateSkipped = ref(false)
  const startupUpdatePhase = ref<'manifest' | 'download' | 'install' | 'skipped'>('manifest')
  const updateSkipCountdown = ref(0)
  const updatePageOpening = ref(false)
  const updatePageError = ref('')
  let updateSkipTimer: number | undefined

  function initialUpdateCheck() {
    return {
      update: undefined,
      channels: [],
      all_failed: false,
      region: 'detecting',
    } as Awaited<ReturnType<typeof api.checkAppUpdate>>
  }

  function finishStartupUpdate() {
    startupUpdateChecking.value = false
    appUpdateCheck.value = null
  }

  function receiveAppUpdateChannel(event: AppUpdateChannelEvent) {
    if (isUnmounted() || !startupUpdateChecking.value) return
    const current = appUpdateCheck.value || initialUpdateCheck()
    const baseChannels = current.region !== event.region
      ? current.channels.filter((channel) => event.region === 'mainland-china' || channel.provider === 'github')
      : current.channels
    const channels = baseChannels.some((channel) => channel.id === event.channel.id)
      ? baseChannels.map((channel) => channel.id === event.channel.id ? event.channel : channel)
      : [...baseChannels, event.channel]
    appUpdateCheck.value = { ...current, region: event.region, channels }
  }

  function receiveAppUpdatePhase(event: AppUpdatePhaseEvent) {
    if (isUnmounted() || !startupUpdateChecking.value) return
    startupUpdatePhase.value = event.phase
    if (event.phase === 'install') startupStage.value = t('正在安装程序更新')
    if (event.update) appUpdate.value = event.update
    if (event.message && event.phase === 'skipped') startupError.value = event.message
  }

  async function checkAppUpdate() {
    const result = await run('正在检查程序更新', () => api.checkAppUpdate(locale.value))
    appUpdateCheck.value = result
    appUpdate.value = result.update || null
    startupUpdatePhase.value = 'manifest'
    if (!result.update && !result.all_failed) showSuccess(t('当前已是最新版本'))
  }

  function clearUpdateSkipTimer() {
    if (updateSkipTimer) window.clearInterval(updateSkipTimer)
    updateSkipTimer = undefined
  }

  async function openManualUpdatePage() {
    if (isUnmounted() || !updateSkipped.value || updatePageOpening.value) return
    clearUpdateSkipTimer()
    updateSkipCountdown.value = 0
    updatePageOpening.value = true
    updatePageError.value = ''
    try {
      await api.openUpdateRepository()
    } catch (reason) {
      if (!isUnmounted() && updateSkipped.value) {
        updatePageError.value = t('无法打开更新页面：{error}')
          .replace('{error}', reason instanceof Error ? reason.message : String(reason))
      }
    } finally {
      updatePageOpening.value = false
    }
  }

  function markUpdateSkipped(message?: string) {
    clearUpdateSkipTimer()
    updateSkipped.value = true
    updateCheckFailed.value = true
    startupUpdatePhase.value = 'skipped'
    startupError.value = message || '请手动更新管理器, 本次跳过更新.'
    updatePageError.value = ''
    updateSkipCountdown.value = 3
    updateSkipTimer = window.setInterval(() => {
      updateSkipCountdown.value -= 1
      if (updateSkipCountdown.value > 0) return
      void openManualUpdatePage()
    }, 1000)
  }

  function continueAfterUpdateSkip() {
    clearUpdateSkipTimer()
    updateSkipped.value = false
    updateCheckFailed.value = false
    startupError.value = ''
    updatePageError.value = ''
    updateSkipCountdown.value = 0
  }

  async function installAppUpdate() {
    startupUpdatePhase.value = 'download'
    beginDownload()
    try {
      const result = await run('正在安装程序更新', () => api.installAppUpdate())
      if (!result.ok && result.message !== '当前已是最新版本') markUpdateSkipped(result.message)
    } finally {
      clearDownload()
    }
  }

  const visibleStartupUpdateChannels = computed(() => appUpdateCheck.value?.channels.filter((channel) => channel.status !== 'skipped') || [])

  return {
    appUpdate, appUpdateCheck, startupUpdateChecking, updateCheckFailed, updateSkipped,
    startupUpdatePhase, updateSkipCountdown, updatePageOpening, updatePageError,
    visibleStartupUpdateChannels, initialUpdateCheck, finishStartupUpdate,
    receiveAppUpdateChannel, receiveAppUpdatePhase, checkAppUpdate, installAppUpdate,
    markUpdateSkipped, continueAfterUpdateSkip, openManualUpdatePage, clearUpdateSkipTimer,
  }
}
