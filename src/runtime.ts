/**
 * 运行环境检测：区分真实 Tauri 窗口与普通浏览器预览。
 *
 * Tauri v2 在 WebView 里注入 `window.__TAURI_INTERNALS__.metadata`；
 * 纯浏览器（Vite 预览、ZCode 内置浏览器）里它是 undefined，
 * 此时调用 getCurrentWindow()/getCurrentWebview() 会直接抛
 * "Cannot read properties of undefined (reading 'metadata')" 并让 setup 崩溃、整页白屏。
 */
export function isTauri(): boolean {
  if (typeof window === 'undefined') return false
  const internals = (window as unknown as { __TAURI_INTERNALS__?: { metadata?: unknown } }).__TAURI_INTERNALS__
  return !!internals?.metadata
}

/** 安全执行 Tauri 专用逻辑：非 Tauri 环境或调用失败时返回 undefined，绝不让调用方崩溃。 */
export async function safeTauri<T>(run: () => Promise<T>): Promise<T | undefined> {
  if (!isTauri()) return undefined
  try {
    return await run()
  } catch {
    return undefined
  }
}

/** 同步版安全执行（用于只读状态获取）。 */
export function safeTauriSync<T>(run: () => T): T | undefined {
  if (!isTauri()) return undefined
  try {
    return run()
  } catch {
    return undefined
  }
}
