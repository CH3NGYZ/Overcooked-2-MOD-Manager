import { PREVIEW_WIDTH, PREVIEW_HEIGHT } from './rendererPreset'

export interface PreviewThumbnail {
  key?: string
  url: string
  state: 'ready' | 'partial' | 'error'
  diagnostics: string[]
  release: () => void
}
interface Entry { url: string; state: 'ready' | 'partial'; diagnostics: string[]; bytes: number; refs: number; used: number; identities: Set<string> }
const entries = new Map<string, Entry>()
const retired = new Set<Entry>()
const presentations = new Map<string, string>()
const MAX_BYTES = 32 * 1024 * 1024
const MAX_PRESENTATIONS_PER_ENTRY = 4
let clock = 0
let trimQueued = false

export function acquireThumbnail(key: string): PreviewThumbnail | null {
  key = presentations.get(key) ?? key
  const entry = entries.get(key)
  if (!entry) return null
  entry.refs += 1
  entry.used = ++clock
  let released = false
  return { key, url: entry.url, state: entry.state, diagnostics: entry.diagnostics, release() {
    if (released) return
    released = true
    entry.refs -= 1
    // A clear may already have installed a new entry under the same key. The
    // old lease releases only its own URL, never the replacement's lifetime.
    if (!entry.refs && (retired.has(entry) || entry.state === 'partial')) {
      if (entries.get(key) === entry) entries.delete(key)
      retired.delete(entry)
      forgetPresentations(entry)
      URL.revokeObjectURL(entry.url)
    }
    // Vue can unmount the source grid before mounting the destination grid.
    // Keep a decoded presentation through that synchronous patch only; no
    // extra lease or timer pins offscreen portraits beyond the microtask.
    // Unrelated releases must respect that handoff too. Partial/retired URL
    // disposal above remains immediate and independent from this global trim.
    if (!entry.refs && entry.identities.size) {
      if (!trimQueued) { trimQueued = true; queueMicrotask(() => { trimQueued = false; trim() }) }
    } else if (!trimQueued) trim()
  } }
}
/** Bind only a successfully decoded presentation, never an in-flight PNG. */
export function presentThumbnail(identity: string, key: string | undefined) {
  if (!key) return
  const entry = entries.get(key)
  if (!entry || entry.state !== 'ready') return
  const previous = presentations.get(identity)
  if (previous) entries.get(previous)?.identities.delete(identity)
  presentations.set(identity, key)
  entry.identities.add(identity)
  // An unchanged fingerprint can survive arbitrarily many editor sessions.
  // Retain only a few recent handoff identities in that existing cache entry.
  while (entry.identities.size > MAX_PRESENTATIONS_PER_ENTRY) {
    const oldest = entry.identities.values().next().value!
    entry.identities.delete(oldest); presentations.delete(oldest)
  }
}
export function putThumbnail(key: string, blob: Blob, state: 'ready' | 'partial', diagnostics: string[]) {
  if (entries.has(key)) return
  entries.set(key, { url: URL.createObjectURL(blob), state, diagnostics,
    bytes: blob.size + PREVIEW_WIDTH * PREVIEW_HEIGHT * 4, refs: 0, used: ++clock, identities: new Set() })
  // The caller leases this new entry after its queued promise resolves. Keep
  // that key through this trim, while reclaiming abandoned older captures.
  trim(key)
}
/** Remove reusable keys immediately; visible consumers retain their old URL
 * until their final lease releases, even if that key is generated again. */
export function clearThumbnailCache() {
  for (const entry of entries.values()) {
    forgetPresentations(entry)
    if (entry.refs) retired.add(entry)
    else URL.revokeObjectURL(entry.url)
  }
  entries.clear()
}
function forgetPresentations(entry: Entry) {
  for (const identity of entry.identities) presentations.delete(identity)
  entry.identities.clear()
}
function trim(pendingKey?: string) {
  let total = [...entries.values(), ...retired].reduce((sum, item) => sum + item.bytes, 0)
  for (const [key, entry] of [...entries].sort((a, b) => a[1].used - b[1].used)) {
    if (total <= MAX_BYTES) break
    if (entry.refs || key === pendingKey) continue
    entries.delete(key)
    forgetPresentations(entry)
    URL.revokeObjectURL(entry.url)
    total -= entry.bytes
  }
}
export function thumbnailCacheStats() {
  const live = [...entries.values(), ...retired]
  return { entries: entries.size, retired: retired.size, referenced: live.filter(item => item.refs > 0).length,
    bytes: live.reduce((sum, item) => sum + item.bytes, 0) }
}
