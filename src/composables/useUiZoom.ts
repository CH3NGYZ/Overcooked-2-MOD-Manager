import { computed, ref } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { api } from '../services/tauri'

/**
 * 界面缩放（内容缩放，不改窗口尺寸）：
 * 写入 WebView 缩放 + 持久化到 config.toml 的 ui_zoom。
 * 范围 1.0–2.0，步进 0.1。
 */
const MIN = 1.0
const MAX = 2.0
const STEP = 0.1
/** 默认缩放 150%（与 v1 一致）。 */
const DEFAULT = 1.5

const initial = Number(localStorage.getItem('oc2-ui-zoom'))
export const zoom = ref<number>(
  Number.isFinite(initial) && initial >= MIN && initial <= MAX ? initial : DEFAULT,
)

export const zoomPercent = computed(() => Math.round(zoom.value * 100))
export const canZoomIn = computed(() => zoom.value < MAX - 1e-6)
export const canZoomOut = computed(() => zoom.value > MIN + 1e-6)

/** 直接设为目标值：先同步更新 ref（UI 立即反映），再异步应用缩放与落盘。 */
export async function setZoom(next: number) {
  const clamped = Math.min(MAX, Math.max(MIN, Math.round(next * 10) / 10))
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

export function zoomIn() { void setZoom(zoom.value + STEP) }
export function zoomOut() { void setZoom(zoom.value - STEP) }
export function resetZoom() { void setZoom(DEFAULT) }

export function useUiZoom() {
  return { zoom, zoomPercent, canZoomIn, canZoomOut, zoomIn, zoomOut, resetZoom, setZoom }
}

// 启动后恢复缩放：优先用后端 config.toml 的 ui_zoom（跨设备/清缓存后仍一致），
// 读不到时退回 localStorage，最后退回默认 150%。
void (async () => {
  try {
    const config = await api.config()
    const value = Number(config?.ui_zoom)
    if (Number.isFinite(value) && value >= MIN && value <= MAX) zoom.value = value
  } catch {
    // 无后端（浏览器预览）时用本地值
  }
  localStorage.setItem('oc2-ui-zoom', String(zoom.value))
  void getCurrentWebview().setZoom(zoom.value).catch(() => undefined)
})()
