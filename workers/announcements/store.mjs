import { byteLength, NOTICE_LIMITS, parseSnapshot } from './feed.mjs'

export async function readSnapshot(database) {
  const row = await database.prepare('SELECT revision, announcements FROM notice_feed WHERE singleton = 1').first()
  const snapshot = row && parseSnapshot(row.revision, row.announcements)
  if (!snapshot) throw new Error('Invalid announcement snapshot')
  return snapshot
}

export async function editSnapshot(database, change) {
  const previous = await readSnapshot(database)
  if (change.revision !== previous.revision) return { status: 409, error: 'conflict' }
  if (previous.revision === Number.MAX_SAFE_INTEGER) throw new Error('Announcement revision exhausted')
  const announcements = previous.announcements
  if (change.action === 'publish') {
    const latest = announcements[0]?.published_at
    const published_at = new Date(Math.max(Date.now(), latest ? Date.parse(latest) : 0)).toISOString()
    announcements.unshift({ id: crypto.randomUUID(), published_at, ...change.draft })
    announcements.length = Math.min(announcements.length, NOTICE_LIMITS.maxAnnouncements)
  } else {
    const index = announcements.findIndex(entry => entry.id === change.id)
    if (index < 0) return { status: 404, error: 'not_found' }
    if (change.action === 'withdraw') announcements.splice(index, 1)
    else announcements[index] = { ...announcements[index], ...change.draft }
  }
  const snapshot = { revision: previous.revision + 1, announcements }
  if (byteLength(JSON.stringify(snapshot)) > NOTICE_LIMITS.payloadBytes) return { status: 400, error: 'invalid_request' }
  const result = await database.prepare('UPDATE notice_feed SET revision = ?, announcements = ? WHERE singleton = 1 AND revision = ?')
    .bind(snapshot.revision, JSON.stringify(announcements), previous.revision).run()
  if (!result.success || ![0, 1].includes(result.meta?.changes)) throw new Error('Announcement update failed')
  return result.meta.changes === 1 ? { snapshot } : { status: 409, error: 'conflict' }
}
