import MarkdownIt from 'markdown-it'
import hljs from 'highlight.js/lib/common'

/**
 * 统一的 Markdown 渲染：markdown-it + highlight.js。
 * 只负责把 Rust 后端返回的 Markdown 文本渲染成 HTML（介绍、更新日志、安装教程等），
 * 不带编辑器，也不接受任何原始 HTML（html: false，从根上避免 XSS）。
 */
const md = new MarkdownIt({
  html: false,        // 不信任源内联 HTML
  linkify: true,      // 裸链接自动成链
  breaks: true,       // 单个换行即换行（备注/日志更符合直觉）
  highlight(code: string, language: string): string {
    // mermaid 交给 renderMermaid() 异步渲染，这里只输出占位容器并保留源码
    if (language === 'mermaid') return ''
    if (language && hljs.getLanguage(language)) {
      try {
        return hljs.highlight(code, { language, ignoreIllegals: true }).value
      } catch {
        // 落到下面自动识别
      }
    }
    return hljs.highlightAuto(code).value
  },
})

/**
 * mermaid 代码块渲染为占位容器：`<pre class="mermaid-block" data-mermaid-source>`。
 * 提前把源码放进 data 属性（已 HTML 转义），避免二次解析 Markdown。
 */
const defaultFence = md.renderer.rules.fence
  ?? ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options))
md.renderer.rules.fence = (tokens, idx, options, env, self) => {
  const token = tokens[idx] as unknown as { info?: string; content?: string }
  const language = (token.info ?? '').trim().split(/\s+/)[0]?.toLowerCase()
  if (language === 'mermaid') {
    const source = (token.content ?? '').replace(/</g, '&lt;').replace(/"/g, '&quot;')
    return `<pre class="mermaid-block" data-mermaid-source="${source}"><code>${token.content ?? ''}</code></pre>`
  }
  return defaultFence(tokens, idx, options, env, self)
}

/* ===== 只放行 <img>，其余 HTML 继续转义 ===== */
/**
 * GitHub 在 issue/评论里复制的图片是 `<img width="…" src="…" />` 形式，
 * 而 `html: false` 会把它们当纯文本显示。这里加一个内联规则，只识别裸 `<img …>`：
 * - src 必须是 http/https（拒绝 data:/blob:/javascript:/file:）；
 * - 只取 alt / title / width / height，其余属性丢弃；
 * - 其他任何 HTML 标签仍由 markdown-it 转义，不引入注入面。
 */
const IMG_TAG = /^<img\b([^>]*)>/i
const ATTR = /([a-zA-Z-]+)\s*=\s*("([^"]*)"|'([^']*)')/g

function readAttributes(source: string): Record<string, string> {
  const attrs: Record<string, string> = {}
  ATTR.lastIndex = 0
  let match: RegExpExecArray | null
  while ((match = ATTR.exec(source)) !== null) {
    const name = match[1].toLowerCase()
    const value = match[3] ?? match[4] ?? ''
    if (!(name in attrs)) attrs[name] = value
  }
  return attrs
}

function isSafeImageUrl(url: string): boolean {
  try {
    const parsed = new URL(url)
    return parsed.protocol === 'https:' || parsed.protocol === 'http:'
  } catch {
    // 相对地址交给 markdown-it 的 baseUrl 处理前先判定为不安全，避免意外拼接
    return false
  }
}

md.inline.ruler.before('html_inline', 'safe_img', (state) => {
  const start = state.pos
  if (state.src.charCodeAt(start) !== 0x3c /* < */) return false
  const rest = state.src.slice(start, start + 4096)
  const match = IMG_TAG.exec(rest)
  if (!match) return false
  const attrs = readAttributes(match[1])
  const src = (attrs.src ?? '').trim()
  if (!src || !isSafeImageUrl(src)) {
    // 不安全：不消费，交给后续规则（html_inline 关闭 → 转义显示）
    return false
  }
  const token = state.push('image', 'img', 0)
  token.attrs = [['src', src]]
  // alt 必须始终存在：markdown-it 默认的 image 渲染器会直接写入 alt 属性
  token.attrs.push(['alt', attrs.alt ?? ''])
  if (attrs.title) token.attrs.push(['title', attrs.title])
  // width/height 必须是纯数字，防注入
  if (/^\d{1,5}$/.test(attrs.width ?? '')) token.attrs.push(['width', attrs.width])
  if (/^\d{1,5}$/.test(attrs.height ?? '')) token.attrs.push(['height', attrs.height])
  token.children = []
  token.content = attrs.alt ?? ''
  state.pos = start + match[0].length
  return true
})

/* markdown-it 的 renderer 规则签名在其类型定义里没有导出，这里用结构化签名标注。 */
type Rule = (
  tokens: Array<{ attrSet: (name: string, value: string) => void }>,
  idx: number,
  options: Record<string, unknown>,
  env: unknown,
  self: { renderToken: (tokens: unknown, idx: number, options: unknown) => string },
) => string

const rules = md.renderer.rules as Record<string, Rule>

// 外链统一新窗口打开，避免抢占 WebView。
const baseLinkOpen = rules.link_open ?? ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options))
rules.link_open = (tokens, idx, options, env, self) => {
  tokens[idx].attrSet('target', '_blank')
  tokens[idx].attrSet('rel', 'noreferrer noopener')
  return baseLinkOpen(tokens, idx, options, env, self)
}

// 图片懒加载
const baseImage = rules.image ?? ((tokens, idx, options, _env, self) => self.renderToken(tokens, idx, options))
rules.image = (tokens, idx, options, env, self) => {
  tokens[idx].attrSet('loading', 'lazy')
  return baseImage(tokens, idx, options, env, self)
}

/** 把 Markdown 渲染成 HTML 字符串。 */
export function renderReadme(markdown: string): string {
  if (!markdown.trim()) return ''
  return md.render(markdown)
}

/** 取 Markdown 的纯文本摘要（卡片简介用，不含任何标记）。 */
export function markdownSummary(markdown: string): string {
  if (!markdown.trim()) return ''
  const html = md.render(markdown)
  if (typeof document === 'undefined') {
    return html.replace(/<[^>]+>/g, ' ').replace(/\s+/g, ' ').trim()
  }
  const holder = document.createElement('div')
  holder.innerHTML = html
  return (holder.textContent ?? '').replace(/\s+/g, ' ').trim()
}

/* ===== Mermaid 图表（按需加载） ===== */

type MermaidApi = {
  initialize: (config: Record<string, unknown>) => void
  render: (id: string, text: string) => Promise<{ svg: string }>
}

let mermaidPromise: Promise<MermaidApi> | null = null
let mermaidCounter = 0

/** 只在页面里真的出现 mermaid 代码块时才加载 mermaid 主包。 */
function loadMermaid(): Promise<MermaidApi> {
  mermaidPromise ??= import('mermaid').then((module) => {
    const api = (module.default ?? module) as unknown as MermaidApi
    api.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: document.documentElement.classList.contains('dark') ? 'dark' : 'default',
      fontFamily: "'Segoe UI', 'Microsoft YaHei', 'PingFang SC', sans-serif",
    })
    return api
  })
  return mermaidPromise
}

/**
 * 把容器内所有 `.mermaid-block` 占位块渲染成 SVG。
 * 放在 Markdown 插入 DOM 之后调用（v-html 更新后 nextTick）。
 * 渲染失败时保留原始代码块，并在下方显示错误说明，不抛异常。
 */
export async function renderMermaidBlocks(root: HTMLElement | null): Promise<void> {
  if (!root) return
  const blocks = Array.from(root.querySelectorAll<HTMLElement>('.mermaid-block[data-mermaid-source]'))
  if (!blocks.length) return
  let api: MermaidApi
  try {
    api = await loadMermaid()
  } catch {
    // 加载失败：保留代码块原文
    return
  }
  for (const block of blocks) {
    if (!block.isConnected) continue
    const source = block.dataset.mermaidSource ?? ''
    if (!source.trim()) continue
    mermaidCounter += 1
    try {
      const { svg } = await api.render(`oc2-mermaid-${mermaidCounter}`, source)
      if (!block.isConnected) continue
      block.classList.add('mermaid-rendered')
      block.innerHTML = svg
    } catch (reason) {
      if (!block.isConnected) continue
      block.classList.add('mermaid-error')
      const message = reason instanceof Error ? reason.message : String(reason)
      const hint = document.createElement('p')
      hint.className = 'mermaid-error-hint'
      hint.textContent = message
      block.after(hint)
    }
  }
}
