import { i18n } from './index.ts'
import { LOCALE_CODES } from './types.ts'

interface TemplateGuard { parts: string[] }
interface LocaleGuards { messages: object; templates: TemplateGuard[] }

// Bounded by the registered five catalogs, never by user-supplied messages.
const catalogs = new Map<string, LocaleGuards>()
const parameter = /\{[A-Za-z][A-Za-z0-9_]*\}/g

function addTemplate(templates: TemplateGuard[], template: string) {
  const parts = template.split(parameter)
  if (parts.length < 2) return
  const fixed = parts.join('')
  // Only simple named interpolation is safe to reconstruct. Do not reinterpret
  // linked/plural/literal syntax or a template consisting only of parameters.
  if (/[{}|]/.test(fixed) || /@[:.]/.test(fixed) || !/\p{L}{2}/u.test(fixed)) return
  templates.push({ parts })
}

function templatesFor(locale: string): TemplateGuard[] {
  const messages = i18n.global.getLocaleMessage(locale)
  const cached = catalogs.get(locale)
  if (cached?.messages === messages) return cached.templates
  const templates: TemplateGuard[] = []
  const visit = (node: unknown, path: string) => {
    if (typeof node === 'string') {
      addTemplate(templates, node)
      // Unknown protocol payloads append opaque details to this fixed message.
      if (path === 'error.backendMessageUnknown') addTemplate(templates, `${node}: {details}`)
    } else if (node && typeof node === 'object') {
      for (const [key, value] of Object.entries(node)) visit(value, path ? `${path}.${key}` : key)
    }
  }
  visit(messages, '')
  catalogs.set(locale, { messages, templates })
  return templates
}

function matches(text: string, { parts }: TemplateGuard): boolean {
  const head = parts[0], tail = parts.at(-1)!
  if (!text.startsWith(head) || !text.endsWith(tail)) return false
  const limit = text.length - tail.length
  let cursor = head.length
  for (const literal of parts.slice(1, -1)) {
    const found = text.indexOf(literal, cursor)
    if (found < 0 || found + literal.length > limit) return false
    cursor = found + literal.length
  }
  return cursor <= limit
}

/** Preserve opaque interpolated values when a translated result crosses another sink. */
export function isRenderedBackendMessage(text: string): boolean {
  return LOCALE_CODES.some(locale => templatesFor(locale).some(template => matches(text, template)))
}
