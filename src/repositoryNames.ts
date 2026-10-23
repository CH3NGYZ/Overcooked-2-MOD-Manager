type LocalizedRepositoryField = Record<string, string | null | undefined> | null | undefined

function localizedRepositoryField(values: LocalizedRepositoryField, language: string): string {
  const localized = [values?.[language], values?.['zh-CN']]
    .find((value): value is string => typeof value === 'string' && !!value.trim())
  return localized?.trim() || ''
}

export type RepositoryNameFields = {
  name_i18n?: LocalizedRepositoryField
}

export type RepositoryNoteFields = {
  note_i18n?: LocalizedRepositoryField
}

export type RepositoryGroupFields = {
  group_i18n?: LocalizedRepositoryField
}

export function repositoryName(repository: RepositoryNameFields, language: string): string {
  return localizedRepositoryField(repository.name_i18n, language)
}

export function repositoryNote(repository: RepositoryNoteFields, language: string): string {
  return localizedRepositoryField(repository.note_i18n, language)
}

export function repositoryGroup(repository: RepositoryGroupFields, language: string): string {
  return localizedRepositoryField(repository.group_i18n, language)
}

export function repositoryGroupKey(repository: RepositoryGroupFields): string {
  return localizedRepositoryField(repository.group_i18n, 'zh-CN')
}
