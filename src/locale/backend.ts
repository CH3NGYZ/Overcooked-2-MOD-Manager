/*
 * 运行时导入必须带 `.ts` 扩展名。
 *
 * 本模块要能被 `node --test` 直接执行（后端 tag 的翻译是纯逻辑，值得单测）。
 * Node 的 ESM 解析不做扩展名补全，而不带扩展名的写法在 Vite 里能过 ——
 * 只有真正跑测试时才会暴露。Vite 也接受显式扩展名，所以两边都能用。
 */
import { i18n } from './index.ts'
import { ERROR_TAG_PREFIX, MESSAGE_TAG_PREFIX } from './types.ts'

/**
 * 后端错误 → 用户可见文案。
 *
 * ## 为什么需要这一层
 *
 * 后端过去直接返回中文（`Err("请打开游戏并确认 街机MOD 正在运行")`），前端
 * 只能原样显示 —— 于是同一条错误在英文界面里仍是中文。根治办法是后端只回
 * **tag**（`error.diyLevel.notRunning`），由这里翻译。
 *
 * 迁移期间两种形态并存：已改造的后端命令返回 tag，未改造的仍返回中文。
 * 本函数两者都接受，所以调用方可以原地替换、逐步推进，不需要等后端一次性改完。
 *
 * ## 约定
 *
 * - tag 形态：`error.<域>.<原因>`，可带参数，形如
 *   `{ tag: 'error.backup.recycleFailed', params: { reason: '...', path: '...' } }`
 * - 为了让后端改动最小，也接受**内联参数**：`error.backup.recycleFailed?path=...`
 *   这种查询串形态，解析后交给 `t()` 插值。
 * - 兜底：既不是已知 tag、也不像中文的，原样返回 —— 宁可显示英文技术信息，
 *   也不要显示一个裸 tag 让人以为程序坏了。
 */
export interface BackendErrorPayload {
  /** `error.*` tag。 */
  tag: string
  /** 插值参数。 */
  params?: Record<string, string | number>
}

/** 判断一个字符串是不是后端 tag（错误或成功提示）。 */
export function isErrorTag(value: string): boolean {
  const tag = value.trim()
  return tag.startsWith(ERROR_TAG_PREFIX) || tag.startsWith(MESSAGE_TAG_PREFIX)
}

/**
 * 解析 `error.xxx.yyy?a=1&b=2` 形态的 tag。
 *
 * 参数放在查询串里是为了让 Rust 侧只用一个 `format!` 就能产出完整的
 * 错误值，不必构造结构体再序列化。
 */
function parseInlineParams(value: string): BackendErrorPayload {
  const [tag, query] = value.split('?', 2)
  if (!query) return { tag }
  const params: Record<string, string> = {}
  for (const pair of query.split('&')) {
    const [rawKey, rawValue] = pair.split('=', 2)
    if (!rawKey) continue
    try {
      params[decodeURIComponent(rawKey)] = decodeURIComponent(rawValue ?? '')
    } catch {
      // 非法百分号编码：保留原文，不让整个提示因为一个参数崩掉
      params[rawKey] = rawValue ?? ''
    }
  }
  return { tag, params }
}

/**
 * 把一个后端返回值翻译成当前语言的文案。
 *
 * 这是所有「显示后端 message」的地方该调的函数 —— 不要直接
 * `message.error(result.message)`，那样会绕过翻译。
 */
export function translateBackendMessage(
  value: string | BackendErrorPayload | null | undefined,
): string {
  if (!value) return ''
  const payload = typeof value === 'string' ? parseInlineParams(value) : value
  const { tag, params } = payload

  /*
   * 只有 `error.*` 才尝试翻译。
   *
   * 其它中文串（成功提示、命令里的普通 message）在迁移完成前原样返回：
   * 它们是后端拼好的整句，没有 tag 可查，硬翻只会翻错。
   */
  if (!isErrorTag(tag)) return tag

  /*
   * vue-i18n 的 t() 在缺 key 时返回 key 本身（本项目关闭了 missingWarn）。
   * 用这一点判断「这个 tag 有没有登记」：没登记就退回 tag 原文，
   * 至少能让开发者看出是哪个 tag 没翻译，而不是显示一句空白。
   */
  const text = params
    ? i18n.global.t(tag, params as Record<string, unknown>)
    : i18n.global.t(tag)
  return text === tag ? tag : text
}

/**
 * 从 `Err` / `message` 里取出可翻译的内容。
 *
 * 前端拿到后端返回时有多种形态：`result.message`（`OperationResult`）、
 * `reason.message`（`Error`）、或直接一个字符串。统一收口到这里，
 * 避免每个调用点各写一遍 `instanceof` 判断。
 */
export function backendMessageOf(reason: unknown): string {
  if (typeof reason === 'string') return translateBackendMessage(reason)
  if (reason instanceof Error) return translateBackendMessage(reason.message)
  if (reason && typeof reason === 'object' && 'message' in reason) {
    const message = (reason as { message?: unknown }).message
    if (typeof message === 'string') return translateBackendMessage(message)
  }
  return translateBackendMessage(String(reason))
}
