import { PREVIEW_WIDTH, PREVIEW_HEIGHT } from './rendererPreset'

export interface PreviewThumbnail {
  key?: string
  url: string
  state: 'ready' | 'partial' | 'error'
  diagnostics: string[]
  release: () => void
}
interface Entry { url: string; state: 'ready' | 'partial'; diagnostics: string[]; bytes: number; refs: number; used: number }
const entries = new Map<string, Entry>()
const retired = new Set<Entry>()
const MAX_BYTES = 32 * 1024 * 1024
let clock = 0

export function acquireThumbnail(key: string): PreviewThumbnail | null {
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
      URL.revokeObjectURL(entry.url)
    }
    trim()
  } }
}
export function putThumbnail(key: string, blob: Blob, state: 'ready' | 'partial', diagnostics: string[]) {
  if (entries.has(key)) return
  entries.set(key, { url: URL.createObjectURL(blob), state, diagnostics,
    bytes: blob.size + PREVIEW_WIDTH * PREVIEW_HEIGHT * 4, refs: 0, used: ++clock })
  // The caller leases this new entry after its queued promise resolves. Keep
  // that key through this trim, while reclaiming abandoned older captures.
  trim(key)
}
/** Remove reusable keys immediately; visible consumers retain their old URL
 * until their final lease releases, even if that key is generated again. */
export function clearThumbnailCache() {
  for (const entry of entries.values()) {
    if (entry.refs) retired.add(entry)
    else URL.revokeObjectURL(entry.url)
  }
  entries.clear()
}
function trim(pendingKey?: string) {
  let total = [...entries.values(), ...retired].reduce((sum, item) => sum + item.bytes, 0)
  for (const [key, entry] of [...entries].sort((a, b) => a[1].used - b[1].used)) {
    if (total <= MAX_BYTES) break
    if (entry.refs || key === pendingKey) continue
    entries.delete(key)
    URL.revokeObjectURL(entry.url)
    total -= entry.bytes
  }
}
export function thumbnailCacheStats() {
  const live = [...entries.values(), ...retired]
  return { entries: entries.size, retired: retired.size, referenced: live.filter(item => item.refs > 0).length,
    bytes: live.reduce((sum, item) => sum + item.bytes, 0) }
}
