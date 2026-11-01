/**
 * 拖放与文件选择共用的扩展名白名单。
 *
 * 为什么单独一个纯逻辑模块：「拖入窗口」与「点击选择文件」必须用同一条判定，
 * 否则同一个文件会在拖动入口被接收、在选择入口被拒绝（或反过来），而用户看到
 * 的是两张卡片给出相反结论。这里不依赖 Vue / Naive / Tauri，Node 测试可以直接
 * import（与 `gameRuntime.ts`、`levelSetStatus.ts` 同一类模块）。
 *
 * `accept` 里的扩展名**不带点**（`['dll', 'zip']`），大小写不敏感。
 */

/** 转义正则元字符：`accept` 现在是代码里的字面量后缀，但不该把「字面量」当前提。 */
function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** 路径是否以 `accept` 里的某个扩展名结尾。空列表一律不接收。 */
export function matchesFileAccept(path: string, accept: string[]): boolean {
  if (!accept.length) return false
  return new RegExp(`\\.(${accept.map(escapeRegExp).join('|')})$`, 'i').test(path)
}

/**
 * 提示文案里的类型列表：`.dll / .zip`。
 *
 * 用语言无关的「/」连接而不是「或 / or」：扩展名本来就是英文，让每种语言各拼
 * 一个连接词只会多出五个容易写错的占位符。
 */
export function acceptLabel(accept: string[]): string {
  return accept.map((extension) => `.${extension}`).join(' / ')
}
