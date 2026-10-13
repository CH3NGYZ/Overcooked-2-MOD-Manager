import DOMPurify from 'dompurify'
import { marked } from 'marked'
import type { Repository } from './types'

export function repositoryMarkdownBaseUrl(repo: Pick<Repository, 'provider' | 'username' | 'repository' | 'branch'>) {
  const host = repo.provider === 'gitee' ? 'gitee.com' : 'github.com'
  const home = `https://${host}/${encodeURIComponent(repo.username)}/${encodeURIComponent(repo.repository)}/`
  const branch = repo.branch?.trim()
  if (!branch || branch.split('/').some((part) => !part || part === '.' || part === '..')) return home
  const path = branch.split('/').map(encodeURIComponent).join('/')
  return `${home}raw/${repo.provider === 'github' ? 'refs/heads/' : ''}${path}/README.md`
}

export function markdownResourceUrl(value: string, baseUrl?: string, image = false): string | undefined {
  const source = value.trim()
  if (!source || /[\u0000-\u001f\u007f]/.test(source)) return undefined
  try {
    const base = baseUrl ? new URL(baseUrl) : undefined
    if (base && (!['http:', 'https:'].includes(base.protocol) || base.username || base.password)) return undefined
    // Repository-root images in the root README belong to its actual Raw branch.
    const relative = image && /^\/[^/]/.test(source) && base?.pathname.includes('/raw/') ? `.${source}` : source
    const url = new URL(relative.startsWith('//') ? `https:${relative}` : relative, base)
    if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password) return undefined
    if (image && url.hostname === 'github.com') url.pathname = url.pathname.replace(/^(\/[^/]+\/[^/]+)\/blob\//, '$1/raw/refs/heads/')
    if (image && url.hostname === 'gitee.com') url.pathname = url.pathname.replace(/^(\/[^/]+\/[^/]+)\/blob\//, '$1/raw/')
    return url.href
  } catch {
    return undefined
  }
}

export function markdownLinkUrl(event: MouseEvent) {
  const anchor = event.target instanceof Element ? event.target.closest('a') : null
  if (!anchor) return undefined
  event.preventDefault()
  const href = anchor.getAttribute('href')
  return href && !href.startsWith('#') ? markdownResourceUrl(href) : undefined
}

export function renderMarkdown(source: string, baseUrl?: string) {
  const html = marked.parse(source, {
    async: false,
    breaks: true,
    gfm: true,
  })
  const fragment = DOMPurify.sanitize(html, {
    RETURN_DOM_FRAGMENT: true,
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
    ALLOWED_TAGS: ['a', 'b', 'blockquote', 'br', 'code', 'dd', 'del', 'details', 'div', 'dl', 'dt', 'em',
      'figcaption', 'figure', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'hr', 'i', 'img', 'kbd', 'li', 'ol',
      'p', 'pre', 's', 'small', 'span', 'strong', 'sub', 'summary', 'sup', 'table', 'tbody', 'td', 'th', 'thead', 'tr', 'ul'],
    ALLOWED_ATTR: ['href', 'src', 'alt', 'title', 'width', 'height', 'align', 'colspan', 'rowspan', 'open'],
  })
  for (const anchor of fragment.querySelectorAll('a[href]')) {
    const href = anchor.getAttribute('href')!
    if (href.startsWith('#')) continue
    const url = markdownResourceUrl(href, baseUrl)
    if (url) anchor.setAttribute('href', url)
    else anchor.removeAttribute('href')
  }
  for (const image of fragment.querySelectorAll('img')) {
    const url = markdownResourceUrl(image.getAttribute('src') || '', baseUrl, true)
    if (url) image.setAttribute('src', url)
    else image.removeAttribute('src')
    for (const dimension of ['width', 'height']) {
      const value = image.getAttribute(dimension)
      if (value && (!/^\d{1,4}$/.test(value) || Number(value) < 1 || Number(value) > 8192)) image.removeAttribute(dimension)
    }
    image.setAttribute('loading', 'lazy')
    image.setAttribute('decoding', 'async')
    image.setAttribute('referrerpolicy', 'no-referrer')
  }
  // Only validated URL/dimension attributes are changed on the sanitized DOM.
  const template = document.createElement('template')
  template.content.append(fragment)
  return template.innerHTML
}
