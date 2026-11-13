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

export function repositoryRawUrl(provider: Provider, owner: string, repository: string, branch: string, path = ''): string {
  const base = provider === 'gitcode'
    ? `https://raw.atomgit.com/${encodeURIComponent(owner)}/${encodeURIComponent(repository)}`
    : repositoryHomepage(provider, owner, repository)
  const ref = provider === 'github' ? 'raw/refs/heads' : 'raw'
  const suffix = [branch, path].filter(Boolean).map(part => part.split('/').map(encodeURIComponent).join('/')).join('/')
  return `${base}/${ref}/${suffix}`
}
