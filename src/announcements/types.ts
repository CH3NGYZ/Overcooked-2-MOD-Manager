import type { Locale } from '../locale/types.ts'

export interface Announcement {
  id: string
  published_at: string
  language: Locale
  title: string
  body: string
}

export interface AnnouncementSnapshot {
  revision: number
  announcements: Announcement[]
}
