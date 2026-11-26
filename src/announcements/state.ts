import { computed, shallowRef } from 'vue'
import protocol from './protocol.json' with { type: 'json' }
import type { AnnouncementSnapshot } from './types.ts'

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

let startupFetch: Promise<void> | undefined

export function initializeAnnouncements(load: () => Promise<AnnouncementSnapshot | null>): Promise<void> {
  startupFetch ??= Promise.resolve().then(load).then(snapshot => {
    announcementSnapshot.value = snapshot
  }).catch(() => { announcementSnapshot.value = null })
  return startupFetch
}

export function markAnnouncementsRead(): void {
  if (!announcementsUnread.value) return
  const displayedIds = announcementSnapshot.value?.announcements.map(({ id }) => id) ?? []
  const ids = [...new Set([...displayedIds, ...readIds.value])].slice(0, READ_LIMIT)
  readIds.value = new Set(ids)
  try { globalThis.localStorage?.setItem(READ_KEY, JSON.stringify(ids)) }
  catch { /* The current window still remembers the read IDs. */ }
}
