import type { ReleaseInfo, Repository } from './types'
import { isCommunitySourceArchive } from './communityRepositories'

export function assetMatchesUi(name: string, rule: { type: string; value: string }) {
  if (rule.type === 'exact') return name.toLocaleLowerCase() === rule.value.toLocaleLowerCase()
  if (rule.type === 'glob') {
    const escaped = rule.value.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.')
    return new RegExp(`^${escaped}$`, 'i').test(name)
  }
  try { return new RegExp(rule.value, 'i').test(name) } catch { return false }
}

export function matchingInstallAssets(repo: Repository, release: ReleaseInfo) {
  if (repo.download_mode === 'repository_file') return []
  return release.assets.filter((asset) => !!asset.download_url && !isCommunitySourceArchive(repo, release, asset.name)
    && repo.asset_rules.some((rule) => assetMatchesUi(asset.name, rule)))
}

export function installReleases(repo: Repository | null) {
  if (!repo || repo.last_error) return []
  return repo.cached_releases.filter((release) => matchingInstallAssets(repo, release).length > 0)
}
