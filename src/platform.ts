/**
 * 窗口外壳所在系统（纯函数，可直接在 Node 里测）。
 *
 * 这里只服务「标题栏窗控按钮摆哪边」这类外壳布局，不参与任何业务判断：
 * 业务上的平台一律读 `config.platform`（后端 `std::env::consts::OS`）。
 *
 * 为什么用 UA 而不是等配置：配置要一次 IPC 才有值，而 macOS 的窗控在左边、
 * Windows 在右边，晚半帧就会看到整排按钮横跳。UA 在首帧就可用。
 *
 * WKWebView 的 UA 形如 `Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) …`，
 * Apple Silicon 也一样写 `Macintosh`。只认这一个词就够：iOS 的 UA 是
 * `CPU iPhone OS 17_0 like Mac OS X`，不带 `Macintosh`，不会被误判。
 */
export function isMacWindowChrome(userAgent: string): boolean {
  return /\bMacintosh\b/i.test(userAgent)
}

/** 当前 WebView 是否是 macOS（取不到 UA 时按 Windows 处理）。 */
export function isMacWindow(): boolean {
  if (typeof navigator === 'undefined') return false
  return isMacWindowChrome(navigator.userAgent)
}
