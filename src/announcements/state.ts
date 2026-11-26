import { computed, shallowRef } from 'vue'
import protocol from './protocol.json' with { type: 'json' }
import type { AnnouncementSnapshot } from './types.ts'
import type { Locale } from '../locale/types.ts'

const READ_KEY = 'oc2-announcement-read-ids'
const READ_LIMIT = protocol.maxItems * 4

function storedReadIds(): Set<string> {
  try {
    const value: unknown = JSON.parse(globalThis.localStorage?.getItem(READ_KEY) ?? '[]')
    if (Array.isArray(value)) {
      return new Set(value.slice(0, READ_LIMIT).filter(id => typeof id === 'string' && id.length === 36))
    }
  } catch { /* An inaccessible or malformed preference cannot block startup. */ }
  return new Set()
}

const readIds = shallowRef(storedReadIds())
export const announcementSnapshot = shallowRef<AnnouncementSnapshot | null | undefined>()
export const announcementsUnread = computed(() =>
  announcementSnapshot.value?.announcements.some(({ id }) => !readIds.value.has(id)) ?? false)

type AnnouncementLoader = (language: Locale, fresh: boolean) => Promise<AnnouncementSnapshot | null>
const pending = new Map<Locale, { fresh: boolean; done: Promise<void> }>()
let current: Promise<void> | undefined

export function loadAnnouncements(language: Locale, load: AnnouncementLoader, fresh = false): Promise<void> {
  let request = pending.get(language)
  if (!request || (fresh && !request.fresh)) {
    const done: Promise<void> = Promise.resolve().then(() => load(language, fresh)).catch(() => null).then(snapshot => {
      if (pending.get(language)?.done === done) pending.delete(language)
      if (current !== done) return
      current = undefined
      announcementSnapshot.value = snapshot
    })
    request = { fresh, done }
    pending.set(language, request)
  }
  if (current !== request.done) {
    current = request.done
    announcementSnapshot.value = undefined
  }
  return request.done
}

export function markAnnouncementsRead(): void {
  if (!announcementsUnread.value) return
  const displayedIds = announcementSnapshot.value?.announcements.map(({ id }) => id) ?? []
  const ids = [...new Set([...displayedIds, ...readIds.value])].slice(0, READ_LIMIT)
  readIds.value = new Set(ids)
  try { globalThis.localStorage?.setItem(READ_KEY, JSON.stringify(ids)) }
  catch { /* The current window still remembers the read IDs. */ }
}
