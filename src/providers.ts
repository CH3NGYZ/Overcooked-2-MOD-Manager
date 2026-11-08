import type { Provider } from './types'

const PLATFORMS: Record<Provider, { label: string; homepage: string }> = {
  github: { label: 'GitHub', homepage: 'https://github.com' },
  gitee: { label: 'Gitee', homepage: 'https://gitee.com' },
  gitcode: { label: 'GitCode', homepage: 'https://gitcode.com' },
}

export function providerLabel(provider: Provider): string {
  return PLATFORMS[provider].label
}

export function repositoryHomepage(provider: Provider, owner: string, repository: string): string {
  return `${ownerHomepage(provider, owner)}/${encodeURIComponent(repository)}`
}

export function ownerHomepage(provider: Provider, owner: string): string {
  return `${PLATFORMS[provider].homepage}/${encodeURIComponent(owner)}`
}

export interface RepositoryCoordinates { provider: Provider; username: string; repository: string }

/** Source document coordinates take precedence over the repositories it lists. */
export function sourceRepository(items: readonly RepositoryCoordinates[], sourceUrl?: string): RepositoryCoordinates | null {
  if (sourceUrl) {
    try {
      const url = new URL(sourceUrl)
      if (url.protocol !== 'https:' || url.username || url.password) return null
      const hosts: Record<string, Provider> = {
        'github.com': 'github', 'raw.githubusercontent.com': 'github', 'api.github.com': 'github',
        'gitee.com': 'gitee', 'gitcode.com': 'gitcode', 'atomgit.com': 'gitcode',
        'raw.atomgit.com': 'gitcode', 'api.atomgit.com': 'gitcode',
      }
      const provider = hosts[url.hostname]
      if (!provider) return null
      const parts = url.pathname.split('/').filter(Boolean).map(decodeURIComponent)
      const offset = parts[0] === 'api' && parts[1] === 'v5' && parts[2] === 'repos' ? 3 : parts[0] === 'repos' ? 1 : 0
      const username = parts[offset], repository = parts[offset + 1]
      if (!username || !repository || [username, repository].some(value => /[\s/\\:?#]/u.test(value) || value === '.' || value === '..')) return null
      if (['search', 'users', 'user', 'api'].includes(username)) return null
      return { provider, username, repository }
    } catch { return null }
  }
  const first = items[0]
  if (!first?.username || !first.repository) return null
  return items.every(item => item.provider === first.provider && item.username.toLowerCase() === first.username.toLowerCase() && item.repository.toLowerCase() === first.repository.toLowerCase()) ? first : null
}

export function repositoryRawUrl(provider: Provider, owner: string, repository: string, branch: string, path = ''): string {
  const base = provider === 'gitcode'
    ? `https://raw.atomgit.com/${encodeURIComponent(owner)}/${encodeURIComponent(repository)}`
    : repositoryHomepage(provider, owner, repository)
  const ref = provider === 'github' ? 'raw/refs/heads' : 'raw'
  const suffix = [branch, path].filter(Boolean).map(part => part.split('/').map(encodeURIComponent).join('/')).join('/')
  return `${base}/${ref}/${suffix}`
}
