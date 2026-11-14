/*
 * 运行时导入必须带 `.ts` 扩展名。
 *
 * 本模块要能被 `node --test` 直接执行（后端 tag 的翻译是纯逻辑，值得单测）。
 * Node 的 ESM 解析不做扩展名补全，而不带扩展名的写法在 Vite 里能过 ——
 * 只有真正跑测试时才会暴露。Vite 也接受显式扩展名，所以两边都能用。
 */
import { i18n } from './index.ts'
import { DOMAINS } from './types.ts'
import { isRenderedBackendMessage } from './renderedMessage.ts'

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
 * - 历史 `tag: 原因` 仅在完整 tag 边界兼容；上下文前缀和嵌套 reason 共用此解析。
 * - 普通文本原样保留；已登记域的未知 tag 用通用文案并保留详情，不输出裸 key。
 */
export interface BackendErrorPayload {
  /** `error.*` tag。 */
  tag: string
  /** 插值参数。 */
  params?: Record<string, string | number>
}

/**
 * 判断一个字符串是不是后端 tag。
 *
 * ## 为什么不再只看前缀
 *
 * 最早的迁移批次把 tag 写成 `error.<域>.<原因>` / `message.<域>.<原因>`，
 * 后来全量迁移统一成「**第一段是域**」的形态（`mods.higherVersionPresent`、
 * `game.notFound`）。两种都合法：`error` / `message` 只是众多域里的两个。
 *
 * 前缀判断因此不可靠 —— 裸域 tag 会被当成普通文本原样漏出去，界面显示
 * `mods.higherVersionPresent`。改为**查表**：语言表里登记过就是 tag。
 * 这样新增域不需要同步维护白名单，也不会因为漏一个前缀而静默失效。
 */
export function isErrorTag(value: string): boolean {
  const tag = value.trim()
  return registeredTag(tag) !== undefined
}

function registeredTag(tag: string): string | undefined {
  if (i18n.global.te(tag)) return tag
  const legacy = tag.startsWith('error.') ? tag.slice('error.'.length) : ''
  return legacy && i18n.global.te(legacy) ? legacy : undefined
}

const domains: ReadonlySet<string> = new Set(DOMAINS)
const tagPrefix = /^([a-z][A-Za-z0-9]*(?:\.[A-Za-z][A-Za-z0-9]*)+)(?=$|[?:：])/
// A reserved domain can also be a filename (config.toml, source.json).
const fileSuffix = /\.(?:toml|json|log|exe|dll|md|zip|png|jpe?g|lock|txt|nsh|wxs|nsi|dylib|cfg|sig|db|app|rs|ts|js|vue)$/i

function reservedTag(tag: string): boolean {
  return domains.has(tag.split('.')[0]) && !fileSuffix.test(tag)
}

function decode(value: string): string {
  try { return decodeURIComponent(value) } catch { return value }
}

/** Split only at the first separator: OS details may themselves contain ? or =. */
function parseInlineParams(query: string): Record<string, string> {
  const params: Record<string, string> = {}
  for (const pair of query.split('&')) {
    const separator = pair.indexOf('=')
    const rawKey = separator < 0 ? pair : pair.slice(0, separator)
    if (!rawKey) continue
    const key = decode(rawKey)
    if (key === '__proto__' || key === 'constructor' || key === 'prototype') continue
    params[key] = decode(separator < 0 ? '' : pair.slice(separator + 1))
  }
  return params
}

function renderPayload(payload: BackendErrorPayload, depth: number): string {
  const key = registeredTag(payload.tag)
  // Only reason can contain another backend error; names/paths/URLs are opaque.
  const params = { ...payload.params }
  if (typeof params.reason === 'string') params.reason = translateText(params.reason, depth + 1)
  if (!key) {
    const details = Object.values(params).map(String).filter(Boolean).join(' · ')
    const fallback = i18n.global.t('error.backendMessageUnknown')
    return details ? `${fallback}: ${details}` : fallback
  }
  const text = i18n.global.t(key, params)
  // Old colon errors also attached details to messages without a reason slot.
  const reason = params.reason
  return reason && !text.includes(String(reason)) ? `${text}: ${reason}` : text
}

/** A whole leading tag only. Never search arbitrary words inside a sentence. */
function translateLeading(value: string, depth: number): string | undefined {
  const match = tagPrefix.exec(value)
  if (!match || !registeredTag(match[1]) && !reservedTag(match[1])) return undefined
  const tag = match[1]
  const tail = value.slice(tag.length)
  const params = tail.startsWith('?')
    ? parseInlineParams(tail.slice(1))
    : tail ? { reason: tail.slice(1).trimStart() } : undefined
  return renderPayload({ tag, params }, depth)
}

function translateText(value: string, depth: number): string {
  if (depth > 12) return value
  // Tauri itself may reject a command missing from the running binary.
  const missingCommand = /^Command (.+) not found$/i.exec(value)
  if (missingCommand) {
    return i18n.global.t('error.commandUnavailable', { command: missingCommand[1] })
  }
  const direct = translateLeading(value, depth)
  if (direct !== undefined) return direct
  // A source name/path/id can itself look like a tag. The catalog's fixed
  // template identifies already-rendered values before context recursion.
  if (isRenderedBackendMessage(value)) return value

  // Source/batch owners may prepend a label. Translate only a complete suffix
  // at an explicit context boundary, preserving every prefix byte and newline.
  const context = /^(.*?(?::[ \t]+|：[ \t]*|\r?\n))([\s\S]+)$/.exec(value)
  if (context) {
    const translated = translateText(context[2], depth + 1)
    if (translated !== context[2]) return context[1] + translated
  }
  return value
}

/** All backend display boundaries use this one parser, including useNotify. */
export function translateBackendMessage(
  value: string | BackendErrorPayload | null | undefined,
): string {
  if (!value) return ''
  return typeof value === 'string' ? translateText(value, 0) : renderPayload(value, 0)
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
  if (reason && typeof reason === 'object') {
    if ('tag' in reason && typeof reason.tag === 'string') {
      const params = 'params' in reason && reason.params && typeof reason.params === 'object'
        ? Object.fromEntries(Object.entries(reason.params).filter(([, value]) => typeof value === 'string' || typeof value === 'number'))
        : undefined
      return translateBackendMessage({ tag: reason.tag, params })
    }
    const message = 'message' in reason ? reason.message : undefined
    if (typeof message === 'string') return translateBackendMessage(message)
  }
  return translateBackendMessage(String(reason))
}
