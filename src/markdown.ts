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
