/**
 * 通知（左下角 toast）的停留时长估算。
 *
 * 为什么需要：Naive 的 `n-message-provider` 只有一个全局 `duration`
 * （默认 3000ms），所有提示都按同一个时长消失。短提示「已保存」3 秒刚好，
 * 但带完整路径的错误可能上百个字符，3 秒根本读不完 —— 用户只能眼看着
 * 它消失，或者被迫去翻日志。
 *
 * 这里按「阅读量」估时长：
 *   · 中日韩字符每个算 1 个单位（信息密度高，一个字符一个音节）
 *   · 拉丁字母、数字、符号每个算 0.5 个单位（单词连读，扫读更快）
 *   · 空白不计数
 *
 * 只做纯计算，不依赖 Vue / Naive，便于直接单测。
 */

/** 基础停留时间：任何提示至少看这么久。 */
const BASE_MS = 3000
/** 前若干个单位不额外加时 —— 3 秒足够读完一句短提示。 */
const FREE_UNITS = 10
/** 超出免费额度后，每个阅读单位额外给的毫秒数。 */
const PER_UNIT_MS = 140
/** 上限：再长也不该一直挂在屏幕上挡住界面。 */
const MAX_MS = 15000

/** 判断是否中日韩字符（含全角标点）。 */
function isCjk(code: number): boolean {
  return (
    (code >= 0x2e80 && code <= 0x9fff) // 部首扩展、CJK 统一表意文字
    || (code >= 0xa960 && code <= 0xa97f) // 谚文字母扩展-A
    || (code >= 0xac00 && code <= 0xd7ff) // 谚文音节、谚文兼容字母
    || (code >= 0xf900 && code <= 0xfaff) // CJK 兼容表意文字
    || (code >= 0xfe30 && code <= 0xfe4f) // CJK 兼容形式
    || (code >= 0xff00 && code <= 0xff60) // 全角 ASCII
    || (code >= 0x20000 && code <= 0x2fa1f) // CJK 扩展 B-F
  )
}

/**
 * 文本的阅读量（单位数）。
 *
 * 注意按**码位**而不是 `length` 遍历：`length` 对扩展区汉字和 emoji
 * 会返回 2，会让时长莫名翻倍。
 */
export function messageReadingUnits(text: string): number {
  let units = 0
  for (const character of text) {
    const code = character.codePointAt(0) ?? 0
    if (/\s/.test(character)) continue
    units += isCjk(code) ? 1 : 0.5
  }
  return units
}

/**
 * 一条通知应该停留多久（毫秒）。
 *
 * @param text 纯文本内容。非字符串（VNode 渲染函数）返回 `undefined`，
 *             由调用方回退到 provider 默认值 —— 那种内容没法估阅读量。
 */
export function messageDuration(text: unknown): number | undefined {
  if (typeof text !== 'string') return undefined
  const units = messageReadingUnits(text)
  const extra = Math.max(0, units - FREE_UNITS) * PER_UNIT_MS
  return Math.min(MAX_MS, Math.round(BASE_MS + extra))
}

/**
 * 是否长到需要给一个手动关闭按钮。
 *
 * 只有长文本才加：短提示带个 × 反而显得吵，而且 3 秒就没了，
 * 用户根本来不及去点。
 */
export function messageNeedsCloseButton(text: unknown): boolean {
  return typeof text === 'string' && messageReadingUnits(text) > 30
}
