import type { Locale } from '../locale/types.ts'

export interface AnnouncementText {
  title: string
  body: string
}

export interface Announcement extends AnnouncementText {
  id: string
  published_at: string
  language: Locale
  translations: Partial<Record<Locale, AnnouncementText>>
}

export interface AnnouncementSnapshot {
  revision: number
  announcements: Announcement[]
}
