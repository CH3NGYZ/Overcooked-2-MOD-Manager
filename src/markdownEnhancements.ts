import DOMPurify from 'dompurify'

const MAX_CODE_LENGTH = 50_000
const MAX_DIAGRAMS = 20
let highlighter: Promise<typeof import('highlight.js/lib/common').default> | undefined
let mermaid: Promise<typeof import('mermaid').default> | undefined
let diagramQueue: Promise<unknown> = Promise.resolve()
let diagramId = 0

function loadMermaid() {
  mermaid ??= import('mermaid').then(({ default: engine }) => {
    engine.initialize({
      startOnLoad: false,
      securityLevel: 'strict',
      theme: 'dark',
      htmlLabels: false,
      flowchart: { htmlLabels: false },
      maxTextSize: MAX_CODE_LENGTH,
      maxEdges: 500,
      suppressErrorRendering: true,
      // Documents cannot override security, resources or rendering configuration.
      secure: ['securityLevel', 'startOnLoad', 'theme', 'htmlLabels', 'flowchart', 'maxTextSize',
        'maxEdges', 'suppressErrorRendering', 'secure'],
    })
    return engine
  }).catch((error: unknown) => { mermaid = undefined; throw error })
  return mermaid
}

function diagramSourceAllowed(source: string) {
  // No init directives or YAML configuration. Mermaid syntax remains untrusted input.
  return source.length <= MAX_CODE_LENGTH && !/^\s*---(?:\r?\n|$)/.test(source) && !/%%\s*\{/.test(source)
    // Resource-loading shapes / CSS are outside this read-only diagram viewer.
    && !/\b(?:img|image)\s*["']?\s*:|url\s*\(|@import|<(?:img|image|iframe|script|style|svg)\b/i.test(source)
}

function diagramImage(svg: string) {
  // Never mount generated SVG, styles, links or bindFunctions into the app DOM.
  // SVG-as-image is isolated from application CSS/JS and cannot navigate or run scripts.
  const clean = DOMPurify.sanitize(svg, {
    USE_PROFILES: { svg: true, svgFilters: true },
    FORBID_TAGS: ['foreignObject', 'script', 'image', 'a'],
    FORBID_ATTR: ['href', 'xlink:href'],
  })
  const parsed = new DOMParser().parseFromString(clean, 'image/svg+xml')
  const root = parsed.documentElement
  if (root.localName !== 'svg' || parsed.querySelector('parsererror')) throw new Error('Invalid SVG')
  const dimensions = root.getAttribute('viewBox')?.trim().split(/[\s,]+/).map(Number)
  if (dimensions?.length === 4 && dimensions.every(Number.isFinite) && dimensions[2]! > 0 && dimensions[3]! > 0) {
    root.setAttribute('width', String(dimensions[2]))
    root.setAttribute('height', String(dimensions[3]))
  }
  return new Blob([new XMLSerializer().serializeToString(root)], { type: 'image/svg+xml' })
}

export interface MarkdownEnhancementOptions {
  signal: AbortSignal
  diagramLabel: string
  diagramError: string
  onLayout: () => void
}

export async function enhanceMarkdown(root: HTMLElement, options: MarkdownEnhancementOptions) {
  const { signal, onLayout } = options
  const active = () => !signal.aborted && root.isConnected
  const codes = Array.from(root.querySelectorAll<HTMLElement>('pre > code'))
  let diagrams = 0
  await Promise.all(codes.map(async (code) => {
    const language = code.className.match(/(?:^|\s)language-([\w+-]+)/)?.[1]?.toLowerCase()
    const source = code.textContent || ''
    if (!language || !active()) return
    if (language !== 'mermaid') {
      if (source.length > MAX_CODE_LENGTH) return
      try {
        highlighter ??= import('highlight.js/lib/common').then((module) => module.default)
          .catch((error: unknown) => { highlighter = undefined; throw error })
        const engine = await highlighter
        if (!active() || !engine.getLanguage(language)) return
        const html = engine.highlight(source, { language, ignoreIllegals: true }).value
        code.innerHTML = DOMPurify.sanitize(html, { ALLOWED_TAGS: ['span'], ALLOWED_ATTR: ['class'] })
        code.classList.add('hljs')
        onLayout()
      } catch { /* Unknown languages or a failed lazy import keep readable source. */ }
      return
    }
    const permitted = ++diagrams <= MAX_DIAGRAMS && diagramSourceAllowed(source)
    const job = diagramQueue.then(async () => {
      if (!active()) return
      let host: HTMLDivElement | undefined
      try {
        if (!permitted) throw new Error('Diagram limit or configuration')
        const engine = await loadMermaid()
        if (!active()) return
        // Mermaid needs layout measurements. Its temporary DOM is removed even on errors.
        host = document.createElement('div')
        host.className = 'markdown-diagram-staging'
        host.inert = true
        document.body.append(host)
        const result = await engine.render(`markdown-diagram-${++diagramId}`, source, host)
        if (!active()) return
        const url = URL.createObjectURL(diagramImage(result.svg))
        signal.addEventListener('abort', () => URL.revokeObjectURL(url), { once: true })
        const image = document.createElement('img')
        image.className = 'markdown-mermaid-image'
        image.alt = options.diagramLabel
        image.src = url
        image.addEventListener('load', onLayout, { once: true })
        image.addEventListener('error', () => {
          URL.revokeObjectURL(url)
          if (active()) {
            image.replaceWith(code.parentElement!)
            showDiagramError(code, options.diagramError)
            onLayout()
          }
        }, { once: true })
        // Retain the original code node until the image has loaded, for failure fallback.
        code.parentElement!.replaceWith(image)
        onLayout()
      } catch {
        if (active()) { showDiagramError(code, options.diagramError); onLayout() }
      } finally { host?.remove() }
    })
    diagramQueue = job.catch(() => undefined)
    await job
  }))
}

function showDiagramError(code: HTMLElement, message: string) {
  const error = document.createElement('p')
  error.className = 'markdown-diagram-error'
  error.setAttribute('role', 'status')
  error.textContent = message
  code.parentElement?.before(error)
}
