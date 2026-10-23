import DOMPurify from 'dompurify'
import { marked } from 'marked'
import GithubSlugger from 'github-slugger'
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
  if (href?.startsWith('#')) {
    // Fragment navigation stays inside this document, never the manager window.
    const root = anchor.closest('.markdown-body')
    let id: string
    try { id = decodeURIComponent(href.slice(1)) } catch { return undefined }
    const prefix = root?.getAttribute('data-markdown-prefix') || 'markdown-'
    const target = root && Array.from(root.querySelectorAll('[id]')).find((element) => element.id === `${prefix}${id}`)
    target?.scrollIntoView({ block: 'start', inline: 'nearest' })
    return undefined
  }
  return href ? markdownResourceUrl(href) : undefined
}

export function renderMarkdown(source: string, baseUrl?: string, idPrefix = 'markdown-', options: { breaks?: boolean } = {}) {
  const html = marked.parse(source, {
    async: false,
    // Repository notes and Release logs are written with single line breaks
    // that authors expect to see, like GitHub comments. README files keep the
    // GitHub rendering where a soft break is a space.
    breaks: options.breaks ?? false,
    gfm: true,
  })
  const fragment = DOMPurify.sanitize(html, {
    RETURN_DOM_FRAGMENT: true,
    ALLOW_DATA_ATTR: false,
    ALLOW_ARIA_ATTR: false,
    ALLOWED_TAGS: ['a', 'b', 'blockquote', 'br', 'code', 'dd', 'del', 'details', 'div', 'dl', 'dt', 'em',
      'figcaption', 'figure', 'h1', 'h2', 'h3', 'h4', 'h5', 'h6', 'hr', 'i', 'img', 'kbd', 'li', 'ol',
      'input', 'p', 'pre', 's', 'small', 'span', 'strong', 'sub', 'summary', 'sup', 'table', 'tbody', 'td', 'th', 'thead', 'tr', 'ul'],
    ALLOWED_ATTR: ['href', 'src', 'alt', 'title', 'width', 'height', 'align', 'colspan', 'rowspan', 'open',
      'class', 'type', 'checked', 'disabled', 'start'],
  })
  // Source HTML cannot inject application classes, IDs or interactive form controls.
  for (const element of fragment.querySelectorAll('[class]')) {
    const language = element.tagName === 'CODE' && element.parentElement?.tagName === 'PRE'
      ? Array.from(element.classList).find((name) => /^language-[\w+-]{1,40}$/.test(name)) : undefined
    element.removeAttribute('class')
    if (language) element.className = language
  }
  for (const input of fragment.querySelectorAll('input')) {
    const item = input.closest('li')
    if (input.getAttribute('type') !== 'checkbox' || !item) {
      input.remove()
      continue
    }
    const checked = input.hasAttribute('checked')
    for (const attribute of Array.from(input.attributes)) input.removeAttribute(attribute.name)
    input.setAttribute('type', 'checkbox')
    input.setAttribute('disabled', '')
    if (checked) input.setAttribute('checked', '')
    input.className = 'markdown-task-checkbox'
    item.classList.add('markdown-task-item')
  }
  const slugger = new GithubSlugger()
  for (const heading of fragment.querySelectorAll('h1,h2,h3,h4,h5,h6')) {
    const slug = slugger.slug(heading.textContent || '')
    heading.id = `${idPrefix}${slug}`
    const permalink = document.createElement('a')
    permalink.className = 'markdown-heading-anchor'
    permalink.href = `#${slug}`
    permalink.textContent = '#'
    permalink.setAttribute('aria-label', heading.textContent || '#')
    heading.prepend(permalink)
  }
  for (const cell of fragment.querySelectorAll('th[align],td[align]')) {
    const align = cell.getAttribute('align')?.toLowerCase()
    cell.removeAttribute('align')
    if (align && ['left', 'center', 'right'].includes(align)) cell.classList.add(`markdown-align-${align}`)
  }
  for (const table of fragment.querySelectorAll('table')) {
    const wrapper = document.createElement('div')
    wrapper.className = 'markdown-table-scroll'
    wrapper.tabIndex = 0
    table.replaceWith(wrapper)
    wrapper.append(table)
  }
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
  // Only validated attributes and locally generated presentation are added after sanitizing.
  const template = document.createElement('template')
  template.content.append(fragment)
  return template.innerHTML
}
