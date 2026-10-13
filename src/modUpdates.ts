import type { InstalledMod, ReleaseInfo, Repository } from './types'

export type ModReleaseState = 'latest' | 'update_available' | 'unknown' | 'repository_unavailable'

export function hasLocalUpdateVersion(mod: InstalledMod) {
  return mod.repository_binding ? !!mod.repository_binding.tag.trim() : !!numericVersion(mod.version)
}

function numericVersion(value?: string) {
  const match = value?.match(/\d+(?:\.\d+){1,3}/)?.[0]
  return match ? match.split('.').map(Number).concat([0, 0, 0, 0]).slice(0, 4) : null
}

function tagVersion(value: string) {
  const match = value.trim().replace(/^[vV]+/, '').match(/(\d+(?:\.\d+){1,3}(?:[-+][0-9A-Za-z.-]+)?)/)?.[1]
  if (!match) return null
  const numeric = match.split(/[-+]/)[0]!
  const parts = numeric.split('.').map(BigInt)
  if (parts.some((part) => part > 18446744073709551615n)) return null
  const suffix = match.slice(numeric.length)
  // Mirror the backend tag normalization, including four-part release tags.
  const normalizedSuffix = parts.length === 4 ? `-${parts[3]}${suffix}` : suffix
  const build = normalizedSuffix.split('+')[1]
  if (build !== undefined && build.split('.').some((part) => !/^[0-9A-Za-z-]+$/.test(part))) return null
  const prerelease = normalizedSuffix.split('+')[0]?.replace(/^-/, '') ?? ''
  const identifiers = prerelease ? prerelease.split('.') : []
  if (identifiers.some((part) => !/^[0-9A-Za-z-]+$/.test(part) || (/^\d+$/.test(part) && /^0\d/.test(part)))) return null
  return { parts: parts.slice(0, 3).concat([0n, 0n, 0n]).slice(0, 3), identifiers }
}

export function compareReleaseTags(remote: string, local: string): number | null {
  const left = tagVersion(remote)
  const right = tagVersion(local)
  if (!left || !right) return null
  for (let index = 0; index < 3; index += 1) {
    if (left.parts[index] !== right.parts[index]) return left.parts[index]! > right.parts[index]! ? 1 : -1
  }
  if (!left.identifiers.length || !right.identifiers.length) {
    return left.identifiers.length === right.identifiers.length ? 0 : left.identifiers.length ? -1 : 1
  }
  for (let index = 0; index < Math.max(left.identifiers.length, right.identifiers.length); index += 1) {
    const a = left.identifiers[index]
    const b = right.identifiers[index]
    if (a === undefined || b === undefined) return a === b ? 0 : a === undefined ? -1 : 1
    if (a === b) continue
    const numericA = /^\d+$/.test(a)
    const numericB = /^\d+$/.test(b)
    if (numericA && numericB) return BigInt(a) > BigInt(b) ? 1 : -1
    if (numericA !== numericB) return numericA ? -1 : 1
    return a > b ? 1 : -1
  }
  return 0
}

export function modRemoteRelease(repo: Repository, mod: InstalledMod): ReleaseInfo | undefined {
  const binding = mod.repository_binding
  if (repo.download_mode !== 'release') return undefined
  if (binding && (repo.provider !== binding.provider
    || repo.username.toLowerCase() !== binding.owner.toLowerCase()
    || repo.repository.toLowerCase() !== binding.repository.toLowerCase()
    || (repo.channel !== binding.channel && repo.channel !== 'all'))) return undefined
  const channel = binding?.channel ?? repo.channel
  return repo.cached_releases?.find((release) => !release.draft
    && (channel === 'all' || (channel === 'beta' ? release.prerelease : !release.prerelease)))
}

export function modRepositoryReleaseState(repo: Repository, mod: InstalledMod): ModReleaseState {
  if (repo.last_error) return 'repository_unavailable'
  const release = modRemoteRelease(repo, mod)
  if (!release) return 'unknown'
  if (mod.repository_binding) {
    const tag = mod.repository_binding.tag
    if (!tag.trim() || !release.tag_name.trim()) return 'unknown'
    if (release.tag_name === tag) return 'latest'
    const compared = compareReleaseTags(release.tag_name, tag)
    return compared === null || compared > 0 ? 'update_available' : 'latest'
  }
  const remote = numericVersion(release.version || release.tag_name)
  const local = numericVersion(mod.version)
  if (!remote || !local) return 'unknown'
  for (let index = 0; index < 4; index += 1) {
    if (remote[index] !== local[index]) return remote[index]! > local[index]! ? 'update_available' : 'latest'
  }
  return 'latest'
}
