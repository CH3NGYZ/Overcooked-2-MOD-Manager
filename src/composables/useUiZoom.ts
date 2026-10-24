import { ref, type Ref } from 'vue'
import type { Dashboard } from '../types'
import { api } from '../services/tauri'
import { t } from '../i18n'

const DEFAULT_UI_ZOOM = 1.5
const MIN_UI_ZOOM = 1
const MAX_UI_ZOOM = 2
const UI_ZOOM_STEP = 0.1

/**
 * UI zoom is content scaling, not window resizing: the value is persisted to
 * config.toml and applied through the --ui-zoom CSS variable on the canvas.
 */
export function useUiZoom(options: { dashboard: Ref<Dashboard | null>; showError: (value: unknown) => void; showSuccess: (value: string) => void }) {
  const { dashboard, showError, showSuccess } = options
  const uiZoom = ref(DEFAULT_UI_ZOOM)
  let saveTimer: number | undefined
  let saveQueue: Promise<void> = Promise.resolve()
  let saveValue = DEFAULT_UI_ZOOM

  function clamp(value: number | undefined) {
    const numeric = Number.isFinite(value) ? value as number : DEFAULT_UI_ZOOM
    return Math.min(MAX_UI_ZOOM, Math.max(MIN_UI_ZOOM, Math.round(numeric * 100) / 100))
  }

  async function setUiZoom(value: number | undefined, persist = true, notify = false) {
    const next = clamp(value)
    uiZoom.value = next
    document.documentElement.style.setProperty('--ui-zoom', String(next))
    if (!persist) {
      if (saveTimer) {
        window.clearTimeout(saveTimer)
        saveTimer = undefined
      }
      saveValue = next
      return
    }
    saveValue = next
    if (saveTimer) window.clearTimeout(saveTimer)
    saveTimer = window.setTimeout(() => {
      saveTimer = undefined
      const valueToSave = saveValue
      saveQueue = saveQueue
        .then(async () => { await api.saveSettings({ ui_zoom: valueToSave }); if (dashboard.value) dashboard.value.config.ui_zoom = valueToSave })
        .catch((reason) => showError(reason))
    }, 250)
    if (notify) {
      showSuccess(t('界面缩放至').replace('{percent}', String(Math.round(next * 100))))
    }
  }

  async function handleWheel(event: WheelEvent) {
    if (!event.ctrlKey || event.deltaY === 0) return
    event.preventDefault()
    const next = uiZoom.value + (event.deltaY < 0 ? UI_ZOOM_STEP : -UI_ZOOM_STEP)
    // Ctrl+wheel changes only the WebView content scale.
    await setUiZoom(next, true, true)
  }

  function cleanup() { if (saveTimer) window.clearTimeout(saveTimer) }

  return { uiZoom, min: MIN_UI_ZOOM, max: MAX_UI_ZOOM, step: UI_ZOOM_STEP, clamp, setUiZoom, handleWheel, cleanup }
}
