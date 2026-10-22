export type RepositoryNameFields = {
  name?: string | null
  name_i18n?: Record<string, string | null | undefined> | null
  username?: string | null
  repository?: string | null
}

export function repositoryName(repository: RepositoryNameFields, language: string): string {
  const localized = [repository.name_i18n?.[language], repository.name_i18n?.['zh-CN'], repository.name]
    .find((value): value is string => typeof value === 'string' && !!value.trim())
  if (localized) return localized.trim()

  return [repository.username, repository.repository]
    .map((value) => value?.trim())
    .filter((value): value is string => !!value)
    .join('/')
}
