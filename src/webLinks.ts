type LinkDestination =
  | { kind: 'internal' }
  | { kind: 'blocked' }
  | { kind: 'anchor'; hash: string }
  | { kind: 'external'; url: string }

export function resolveWebLink(href: string, page: URL): LinkDestination {
  let target: URL
  try {
    target = new URL(href, page)
  } catch {
    return { kind: 'blocked' }
  }
  if (target.username || target.password) return { kind: 'blocked' }
  // WKWebView's tauri:// origin is opaque to URL.origin; compare its authority as well.
  if (target.protocol === page.protocol && target.host === page.host) {
    if (target.pathname === page.pathname && target.search === page.search
      && target.href.includes('#') && !target.hash.startsWith('#/')) {
      return { kind: 'anchor', hash: target.hash.slice(1) }
    }
    return { kind: 'internal' }
  }
  return target.protocol === 'http:' || target.protocol === 'https:'
    ? { kind: 'external', url: target.href }
    : { kind: 'blocked' }
}

/** One listener for the main document, including modals teleported outside #app. */
export function listenForWebLinks(
  root: Document,
  open: (url: string) => void,
  blocked: () => void,
): () => void {
  function activate(event: MouseEvent) {
    if (event.defaultPrevented || event.button !== (event.type === 'auxclick' ? 1 : 0)) return
    const link = event.target instanceof Element ? event.target.closest('a[href]') : null
    const href = link?.getAttribute('href')
    if (!href) return
    const destination = resolveWebLink(href, new URL(root.URL))
    if (destination.kind === 'internal') return
    event.preventDefault()
    if (!event.isTrusted) return
    if (destination.kind === 'external') open(destination.url)
    else if (destination.kind === 'blocked') blocked()
    else if (destination.hash) {
      // Scrolling without changing the hash keeps Vue Router on the current route.
      let id = destination.hash
      try { id = decodeURIComponent(id) } catch { /* Try the literal id. */ }
      root.getElementById(id)?.scrollIntoView({ block: 'nearest' })
    }
  }
  root.addEventListener('click', activate)
  root.addEventListener('auxclick', activate)
  return () => {
    root.removeEventListener('click', activate)
    root.removeEventListener('auxclick', activate)
  }
}
