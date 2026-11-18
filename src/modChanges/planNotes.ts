import { pickI18n } from '../i18n'
import type { ChangeInstallNote } from './types'

export interface InstallNote { id: string; name: string; text: string }

export function installNoteKey({ id, text }: InstallNote): string {
  return JSON.stringify([id, text])
}

/** Localize the plan's selected notes without consulting the live repository list. */
export function localizedInstallNotes(notes: readonly ChangeInstallNote[]): InstallNote[] {
  return notes.flatMap(({ id, name, name_i18n, note, note_i18n }) => {
    const text = pickI18n(note_i18n, note).trim()
    return text ? [{ id, name: pickI18n(name_i18n, name), text }] : []
  })
}
