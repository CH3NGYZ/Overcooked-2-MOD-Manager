import type { Provider, PublicConfig, ReleaseInfo, Repository } from './types'

export const COMMUNITY_SOURCE_ID = 'github-community'
export const GITEE_COMMUNITY_SOURCE_ID = 'gitee-community'
export const GITEE_COMMUNITY_TOPIC_URL = 'https://gitee.com/explore/topic/overcooked-2'
export const COMMUNITY_PAGE_SIZE = 10

export function communitySourceId(provider: Provider) {
  return provider === 'gitee' ? GITEE_COMMUNITY_SOURCE_ID : COMMUNITY_SOURCE_ID
}

export function communityProviderMatchesRegion(provider: Provider, region: string | undefined) {
  return provider !== 'gitee' || region === 'chinese_mainland'
}

export function isCommunitySourceArchive(repo: Repository, release: ReleaseInfo, assetName: string) {
  return repo.provider === 'gitee' && isCommunityRepository(repo, 'gitee')
    && !!release.tag_name.trim() && assetName.trim().toLowerCase() === `${release.tag_name.trim()}.zip`.toLowerCase()
}

export function isCommunitySource(sourceId?: string) {
  return sourceId === COMMUNITY_SOURCE_ID || sourceId === GITEE_COMMUNITY_SOURCE_ID
}

export function isCommunityRepository(repo: Repository, provider?: Provider) {
  return provider ? repo.source_id === communitySourceId(provider) : isCommunitySource(repo.source_id)
}

export function communityDiscovery(config: PublicConfig | undefined | null, provider: Provider) {
  return provider === 'gitee' ? config?.gitee_community_discovery : config?.community_discovery
}

function identity(repo: Repository) {
  return `${repo.provider}:${repo.username.toLowerCase()}/${repo.repository.toLowerCase()}`
}

function officialRepositories(repositories: Repository[]) {
  return repositories.filter((repo) => repo.builtin && !isCommunityRepository(repo))
}

export function orderedCommunityRepositories(config?: PublicConfig | null, provider: Provider = 'github') {
  if (!communityProviderMatchesRegion(provider, config?.region)) return []
  const all = config?.repositories ?? []
  const official = new Set(officialRepositories(all).map(identity))
  const repositories = all.filter((repo) => isCommunityRepository(repo, provider) && !official.has(identity(repo)))
  const byId = new Map(repositories.map((repo) => [repo.id, repo]))
  const found = communityDiscovery(config, provider)?.repository_ids ?? []
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
