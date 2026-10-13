import type { PublicConfig, Repository } from './types'

export const COMMUNITY_SOURCE_ID = 'github-community'
export const COMMUNITY_PAGE_SIZE = 10

export function isCommunityRepository(repo: Repository) {
  return repo.source_id === COMMUNITY_SOURCE_ID
}

function identity(repo: Repository) {
  return `${repo.provider}:${repo.username.toLowerCase()}/${repo.repository.toLowerCase()}`
}

function officialRepositories(repositories: Repository[]) {
  return repositories.filter((repo) => repo.builtin && repo.provider === 'github' && !isCommunityRepository(repo))
}

export function orderedCommunityRepositories(config?: PublicConfig | null) {
  const all = config?.repositories ?? []
  const official = new Set(officialRepositories(all).map(identity))
  const repositories = all.filter((repo) => isCommunityRepository(repo) && !official.has(identity(repo)))
  const byId = new Map(repositories.map((repo) => [repo.id, repo]))
  const found = config?.community_discovery?.repository_ids ?? []
  const ids = new Set(found)
  return [
    ...[...ids].flatMap((id) => byId.has(id) ? [byId.get(id)!] : []),
    ...repositories.filter((repo) => !ids.has(repo.id)),
  ]
}

export function communityNavigationTargets(repositories: Repository[], targets: Set<string>) {
  const community = new Set(repositories.filter((repo) => isCommunityRepository(repo) && targets.has(repo.id)).map(identity))
  return new Set([...targets, ...officialRepositories(repositories).filter((repo) => community.has(identity(repo))).map((repo) => repo.id)])
}

export function communityPageCount(repositories: Repository[]) {
  return Math.max(1, Math.ceil(repositories.length / COMMUNITY_PAGE_SIZE))
}

export function communityPage(repositories: Repository[], page: number) {
  const current = Math.max(1, Math.min(page, communityPageCount(repositories)))
  return repositories.slice((current - 1) * COMMUNITY_PAGE_SIZE, current * COMMUNITY_PAGE_SIZE)
}
