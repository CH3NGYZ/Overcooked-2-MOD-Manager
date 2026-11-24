import { readonly, shallowRef } from 'vue'
import { MAX_NAME_SIZE, MIN_NAME_SIZE, buildShortestRichText, isNameOverLimit, styleName } from '../colorfulName.ts'

export const NAME_HISTORY_LIMIT = 20
const STORAGE_KEY = 'oc2-colorful-name-history'

export interface NameDraft {
  name: string
  colors: readonly string[]
  sizes: readonly number[]
}

interface HistoryEntry extends NameDraft { key: string }
export type NameAction = 'copy' | 'push'

function readDraft(value: unknown): NameDraft | undefined {
  if (!value || typeof value !== 'object' || !('name' in value) || typeof value.name !== 'string'
    || !value.name.trim() || isNameOverLimit(value.name)) return
  if (!('colors' in value) || !Array.isArray(value.colors) || !('sizes' in value) || !Array.isArray(value.sizes)) return
  const count = [...value.name].length
  if (value.colors.length !== count || value.sizes.length !== count) return
  if (!value.colors.every(color => typeof color === 'string' && /^#[0-9a-f]{6}$/i.test(color))
    || !value.sizes.every(size => typeof size === 'number' && Number.isFinite(size) && size >= MIN_NAME_SIZE && size <= MAX_NAME_SIZE)) return
  return { name: value.name, colors: value.colors.map(color => color.toLowerCase()), sizes: [...value.sizes] }
}

function entryFor(draft: NameDraft): HistoryEntry {
  return { ...draft, key: JSON.stringify([draft.name, draft.colors, draft.sizes]) }
}

export function createNameHistory(storage: () => Pick<Storage, 'getItem' | 'setItem' | 'removeItem'> = () => globalThis.localStorage) {
  function load(): HistoryEntry[] {
    try {
      const stored: unknown = JSON.parse(storage().getItem(STORAGE_KEY) ?? 'null')
      if (!Array.isArray(stored)) return []
      const entries = new Map<string, HistoryEntry>()
      for (const item of stored) {
        const draft = readDraft(item)
        if (!draft || isNameOverLimit(buildShortestRichText(styleName(draft.name, draft.colors, draft.sizes)))) continue
        const entry = entryFor(draft)
        if (!entries.has(entry.key)) entries.set(entry.key, entry)
        if (entries.size === NAME_HISTORY_LIMIT) break
      }
      return [...entries.values()]
    } catch { return [] }
  }

  const entries = shallowRef(load())
  const busy = shallowRef<NameAction | undefined>()
  let pending: { key: string; dismissed: boolean } | undefined

  function save(next: HistoryEntry[]) {
    entries.value = next
    try {
      if (next.length) storage().setItem(STORAGE_KEY, JSON.stringify(next.map(({ name, colors, sizes }) => ({ name, colors, sizes }))))
      else storage().removeItem(STORAGE_KEY)
    } catch { return }
  }

  function remove(key: string) {
    if (pending?.key === key) pending.dismissed = true
    save(entries.value.filter(entry => entry.key !== key))
  }

  function clear() {
    if (pending) pending.dismissed = true
    save([])
  }

  async function submit<Result extends { ok: boolean }>(action: NameAction, draft: NameDraft,
    send: (code: string) => Promise<Result>): Promise<Result | undefined> {
    if (pending) return
    const snapshot = readDraft(draft)
    if (!snapshot) return
    const code = buildShortestRichText(styleName(snapshot.name, snapshot.colors, snapshot.sizes))
    if (isNameOverLimit(code)) return
    const entry = entryFor(snapshot)
    const receipt = pending = { key: entry.key, dismissed: false }
    busy.value = action
    try {
      const result = await send(code)
      if (result.ok === true && !receipt.dismissed) {
        save([entry, ...entries.value.filter(item => item.key !== entry.key)].slice(0, NAME_HISTORY_LIMIT))
      }
      return result
    } finally {
      pending = undefined
      busy.value = undefined
    }
  }

  return { entries: readonly(entries), busy: readonly(busy), submit, remove, clear }
}

export const nameHistory = createNameHistory()
