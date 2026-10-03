import DOMPurify from 'dompurify'
import { marked } from 'marked'

function escapeHtml(value: string) {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

const renderer = new marked.Renderer()
renderer.html = ({ text }) => escapeHtml(text)

export function renderMarkdown(source: string) {
  const html = marked.parse(source, {
    async: false,
    breaks: true,
    gfm: true,
    renderer,
  })
  return DOMPurify.sanitize(html, {
    ALLOW_DATA_ATTR: false,
    FORBID_TAGS: ['audio', 'embed', 'form', 'iframe', 'img', 'object', 'script', 'source', 'style', 'svg', 'video'],
  })
}
