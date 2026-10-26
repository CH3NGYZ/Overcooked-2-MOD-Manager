import { computed, ref } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { api } from '../services/tauri'
import { safeTauri } from '../runtime'

/**
 * 界面缩放（内容缩放，不改窗口尺寸）：
 * 写入 WebView 缩放 + 持久化到 config.toml 的 ui_zoom。
 * 档位固定为 100/125/150/175/200（25% 步进），默认 150%。
 */
const MIN = 1.0
const MAX = 2.0
/** 可选档位：下拉与取整都以它为准，避免 1.25 被四舍五入成 1.3。 */
export const ZOOM_STEPS = [1, 1.25, 1.5, 1.75, 2] as const
const DEFAULT = 1.5

/** 把任意值吸附到最近的合法档位。 */
function snap(value: number): number {
  let best: number = ZOOM_STEPS[0]
  let bestDistance = Number.POSITIVE_INFINITY
  for (const step of ZOOM_STEPS) {
    const distance = Math.abs(step - value)
    if (distance < bestDistance) {
      bestDistance = distance
      best = step
    }
  }
  return best
}

const initial = Number(localStorage.getItem('oc2-ui-zoom'))
export const zoom = ref<number>(
  Number.isFinite(initial) ? snap(initial) : DEFAULT,
)

export const zoomPercent = computed(() => Math.round(zoom.value * 100))
export const canZoomIn = computed(() => zoom.value < MAX - 1e-6)
export const canZoomOut = computed(() => zoom.value > MIN + 1e-6)

/** 直接设为目标值：先同步更新 ref（UI 立即反映），再异步应用缩放与落盘。 */
export async function setZoom(next: number) {
  const clamped = snap(Math.min(MAX, Math.max(MIN, next)))
  zoom.value = clamped
  localStorage.setItem('oc2-ui-zoom', String(clamped))
  try {
    await getCurrentWebview().setZoom(clamped)
  } catch {
    // 浏览器预览没有 webview 权限：仅保留本地值
  }
  try {
    await api.saveSettings({ ui_zoom: clamped })
  } catch {
    // 无后端时忽略持久化失败
  }
}

export function zoomIn() {
  const index = ZOOM_STEPS.indexOf(zoom.value as (typeof ZOOM_STEPS)[number])
  void setZoom(ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, index + 1)])
}
export function zoomOut() {
  const index = ZOOM_STEPS.indexOf(zoom.value as (typeof ZOOM_STEPS)[number])
  void setZoom(ZOOM_STEPS[Math.max(0, index - 1)])
}
export function resetZoom() { void setZoom(DEFAULT) }

export function useUiZoom() {
  return { zoom, zoomPercent, canZoomIn, canZoomOut, zoomIn, zoomOut, resetZoom, setZoom }
}

// 启动后恢复缩放：优先用后端 config.toml 的 ui_zoom（跨设备/清缓存后仍一致），
// 读不到时退回 localStorage，最后退回默认 150%。旧配置里的 1.3/1.8 会吸附到合法档位。
void (async () => {
  try {
    const config = await api.config()
    const value = Number(config?.ui_zoom)
    if (Number.isFinite(value)) zoom.value = snap(value)
  } catch {
    // 无后端（浏览器预览）时用本地值
  }
  localStorage.setItem('oc2-ui-zoom', String(zoom.value))
  await safeTauri(async () => getCurrentWebview().setZoom(zoom.value))
})()
