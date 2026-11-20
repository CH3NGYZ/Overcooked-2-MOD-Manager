import { localVersion, matchesRepositoryIdentity } from './modStatus.ts'
import type { InstalledMod, ModRepository, PublicConfig } from './types'

/** Release freshness and display data normally do not change local DLL ownership. */
function declaration(repo: ModRepository) {
  const { cached_releases, cached_releases_complete, release_prerelease_metadata,
    releases_cached_at, etag, last_error, ...rest } = repo
  return JSON.stringify(rest)
}

function platformVerdict(repo: ModRepository, tag: string): boolean | undefined {
  return repo.release_prerelease_metadata?.[tag]
    ?? repo.cached_releases?.find(release => release.tag_name === tag)?.apiPrerelease ?? undefined
}

/** Decide when to ask Rust to rebind, without deciding how to resolve the binding. */
export function modBindingInputsChanged(
  before: PublicConfig | undefined, after: PublicConfig, installed: readonly InstalledMod[],
): boolean {
  if (!before || before.region !== after.region || before.platform !== after.platform) return true
  if (before.repository_sources !== after.repository_sources
    && JSON.stringify(before.repository_sources) !== JSON.stringify(after.repository_sources)) return true
  if (before.repositories === after.repositories) return false
  const previous = before.repositories ?? []
  const next = after.repositories ?? []
  if (previous.length !== next.length) return true
  return next.some((repo, i) => {
    const old = previous[i]!
    if (old === repo) return false
    if (declaration(old) !== declaration(repo)) return true
    if (repo.download_mode !== 'release') return false
    return installed.some(mod => {
      const binding = mod.repository_binding
      if (!binding || localVersion(mod) !== binding.tag || !matchesRepositoryIdentity(repo, binding)) return false
      const raw = platformVerdict(repo, binding.tag)
      // Raw false is evidence too. A renewed verdict may also outrank one from
      // another channel of this upstream; let Rust reconcile those timestamps.
      return raw !== platformVerdict(old, binding.tag)
        || (raw !== undefined && raw !== mod.prerelease && repo.last_error == null
          && (repo.releases_cached_at !== old.releases_cached_at || repo.cached_releases !== old.cached_releases))
    })
  })
}
