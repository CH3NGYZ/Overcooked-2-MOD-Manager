import { computed, ref } from 'vue'
import { getCurrentWebview } from '@tauri-apps/api/webview'
import { api } from '../services/tauri'

/**
 * 界面缩放（内容缩放，不改窗口尺寸）：
 * 写入 WebView 缩放 + 持久化到 config.toml 的 ui_zoom。
 * 范围 1.0–2.0，步进 0.1，与 v1 约定一致。
 */
const MIN = 1.0
const MAX = 2.0
const STEP = 0.1

const initial = Number(localStorage.getItem('oc2-ui-zoom'))
const zoom = ref(Number.isFinite(initial) && initial >= MIN && initial <= MAX ? initial : 1.0)

export const zoomPercent = computed(() => Math.round(zoom.value * 100))
export const canZoomIn = computed(() => zoom.value < MAX - 1e-6)
export const canZoomOut = computed(() => zoom.value > MIN + 1e-6)

async function apply(next: number) {
  const clamped = Math.min(MAX, Math.max(MIN, Math.round(next * 10) / 10))
  if (clamped === zoom.value) return
  zoom.value = clamped
  localStorage.setItem('oc2-ui-zoom', String(clamped))
  try {
    await getCurrentWebview().setZoom(clamped)
    await api.saveSettings({ ui_zoom: clamped })
  } catch {
    // 无后端（浏览器预览）时仅保留本地缩放
  } finally {
    void 0
  }
}

export function useUiZoom() {
  function zoomIn() { void apply(zoom.value + STEP) }
  function zoomOut() { void apply(zoom.value - STEP) }
  function resetZoom() { void apply(1) }
  function setZoom(value: number) { void apply(value) }
  return { zoom, zoomPercent, canZoomIn, canZoomOut, zoomIn, zoomOut, resetZoom, setZoom }
}

// 启动时恢复缩放（等 WebView 就绪）
void getCurrentWebview().setZoom(zoom.value).catch(() => undefined)
