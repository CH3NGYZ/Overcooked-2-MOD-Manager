/**
 * 极简 Markdown → HTML（README 展示用）：
 * 只覆盖常见语法（标题、段落、列表、代码块、行内代码、链接、粗体、引用、分割线），
 * 先整体转义，再按行解析，避免任何 HTML 注入。不追求完整 GFM。
 */

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
}

/** 行内：代码、链接、粗体、斜体。输入必须已转义。 */
function inline(text: string): string {
  let out = text
  // 行内代码优先，避免内部被后续规则破坏
  const codes: string[] = []
  out = out.replace(/`([^`]+)`/g, (_m, code: string) => {
    codes.push(`<code>${code}</code>`)
    return `\u0000${codes.length - 1}\u0000`
  })
  // 图片与链接（仅 http/https）
  out = out.replace(/!\[([^\]]*)\]\((https?:\/\/[^\s)]+)\)/g,
    (_m, alt: string, url: string) => `<img src="${url}" alt="${alt}" loading="lazy" />`)
  out = out.replace(/\[([^\]]+)\]\((https?:\/\/[^\s)]+)\)/g,
    (_m, label: string, url: string) => `<a href="${url}" target="_blank" rel="noreferrer">${label}</a>`)
  out = out.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>')
  out = out.replace(/(^|[^*])\*([^*]+)\*/g, '$1<em>$2</em>')
  out = out.replace(/\u0000(\d+)\u0000/g, (_m, i: string) => codes[Number(i)] ?? '')
  return out
}

/** 把 Markdown 渲染为安全 HTML 字符串。 */
export function renderReadme(markdown: string): string {
  if (!markdown.trim()) return ''
  const lines = escapeHtml(markdown.replace(/\r\n?/g, '\n')).split('\n')
  const out: string[] = []
  let inCode = false
  let listType: 'ul' | 'ol' | null = null

  const closeList = () => {
    if (listType) {
      out.push(`</${listType}>`)
      listType = null
    }
  }

  for (const rawLine of lines) {
    const line = rawLine.trimEnd()

    // 代码块围栏
    if (/^```/.test(line)) {
      if (inCode) {
        out.push('</code></pre>')
        inCode = false
      } else {
        closeList()
        out.push('<pre><code>')
        inCode = true
      }
      continue
    }
    if (inCode) {
      out.push(line)
      continue
    }

    if (!line.trim()) {
      closeList()
      continue
    }

    // 分割线
    if (/^(-{3,}|\*{3,}|_{3,})$/.test(line.trim())) {
      closeList()
      out.push('<hr />')
      continue
    }

    // 标题
    const heading = line.match(/^(#{1,6})\s+(.*)$/)
    if (heading) {
      closeList()
      const level = heading[1].length
      out.push(`<h${level}>${inline(heading[2])}</h${level}>`)
      continue
    }

    // 引用
    const quote = line.match(/^&gt;\s?(.*)$/)
    if (quote) {
      closeList()
      out.push(`<blockquote>${inline(quote[1])}</blockquote>`)
      continue
    }

    // 无序列表
    const bullet = line.match(/^\s*[-*+]\s+(.*)$/)
    if (bullet) {
      if (listType !== 'ul') {
        closeList()
        out.push('<ul>')
        listType = 'ul'
      }
      out.push(`<li>${inline(bullet[1])}</li>`)
      continue
    }

    // 有序列表
    const ordered = line.match(/^\s*\d+[.)]\s+(.*)$/)
    if (ordered) {
      if (listType !== 'ol') {
        closeList()
        out.push('<ol>')
        listType = 'ol'
      }
      out.push(`<li>${inline(ordered[1])}</li>`)
      continue
    }

    // 表格：跳过（README 里少见，退化成段落更安全）
    closeList()
    out.push(`<p>${inline(line)}</p>`)
  }

  if (inCode) out.push('</code></pre>')
  closeList()
  return out.join('\n')
}
