import { isTauri } from './runtime'

/**
 * 系统明暗外观的唯一读取口（`themeMode === 'auto'` 时用它决定实际用哪套配色）。
 *
 * ## 两个平台怎么报「现在是白天还是黑夜」
 *
 * 两个平台的 WebView 支持的都是 `prefers-color-scheme`，但背后的开关不同：
 *
 * - **Windows（WebView2）**：跟随「设置 → 个性化 → 颜色 → 应用模式」。
 *   只要本应用没有调用过 Tauri 的 `Window.setTheme()`，这个媒体查询就是系统的
 *   原样值；用户改设置时媒体查询立刻变，同时宿主会先发 `tauri://theme-changed`。
 * - **macOS（WKWebView）**：跟随 `NSApp.effectiveAppearance` —— 也就是「外观」
 *   里选的浅色/深色/自动（自动时按日出日落切）。macOS 10.14 起才有外观概念，
 *   更老的系统一律 `light`。
 *
 * 差异只在**谁先到**：Windows 上宿主的 theme-changed 往往早于媒体查询更新；
 * macOS 上媒体查询本身就是系统级信号。两条路都接、谁先到用谁，
 * 读不到（老 macOS、浏览器预览）时按浅色处理。
 *
 * 注意：这里**只读**系统外观，不调用 `setTheme()` 去覆盖窗口主题 ——
 * 一旦调用过，WebView2 的 `prefers-color-scheme` 就变成被覆盖后的值，
 * 「跟随系统」会退化成「跟随上次手动选的颜色」。
 */
export function systemPrefersDark(): boolean {
  if (typeof window === 'undefined' || typeof window.matchMedia !== 'function') return false
  return window.matchMedia('(prefers-color-scheme: dark)').matches
}

/**
 * 订阅系统外观变化。
 *
 * 返回取消函数；`theme.ts` 在模块级调用一次并持有到进程结束，
 * 单个组件不需要各自订阅。
 */
export function watchSystemTheme(onChange: (dark: boolean) => void): () => void {
  const cleanups: Array<() => void> = []

  if (typeof window !== 'undefined' && typeof window.matchMedia === 'function') {
    const query = window.matchMedia('(prefers-color-scheme: dark)')
    const listener = (event: MediaQueryListEvent) => onChange(event.matches)
    query.addEventListener('change', listener)
    cleanups.push(() => query.removeEventListener('change', listener))
  }

  if (isTauri()) {
    let alive = true
    void (async () => {
      try {
        const { getCurrentWindow } = await import('@tauri-apps/api/window')
        const unlisten = await getCurrentWindow().onThemeChanged((event) => {
          if (alive) onChange(event.payload === 'dark')
        })
        if (alive) cleanups.push(unlisten)
        else unlisten()
      } catch {
        // 宿主不支持该事件（或权限未开）：媒体查询那条路已经能覆盖
      }
    })()
    cleanups.push(() => {
      alive = false
    })
  }

  return () => {
    for (const cleanup of cleanups.splice(0)) cleanup()
  }
}
