import { selectReleaseNotes } from './appUpdateNotes.ts'
import type { Locale } from './locale/types.ts'
import { assetVersionIndex } from './mapVersion.ts'
import { compareVersions, isPrereleaseVersion, localVersion, matchesRepositoryIdentity, modsForRepository } from './modStatus.ts'
import type { InstalledMod, LevelSetInfo, MapInfo, ModRepository, ReleaseInfo } from './types.ts'

type CompareTags = (left: string, right: string) => number | null
export type PackageReleaseNote = { tag: string; body: string; prerelease: boolean }

function note(release: ReleaseInfo): PackageReleaseNote {
  return {
    tag: release.tag_name,
    body: release.body,
    prerelease: isPrereleaseVersion(release.tag_name, release.apiPrerelease, release.prerelease),
  }
}

export function packageReleaseNotes(
  releases: readonly ReleaseInfo[], target: ReleaseInfo, installedTag?: string | null, compare: CompareTags = compareVersions,
): PackageReleaseNote[] {
  if (target.draft) return []
  const selected = note(target)
  if (!installedTag || (compare(target.tag_name, installedTag) ?? 0) <= 0) return [selected]

  const seen = new Set([selected.tag])
  const history: PackageReleaseNote[] = []
  for (const release of releases) {
    if (release.draft || !release.body.trim() || seen.has(release.tag_name)) continue
    const afterInstalled = compare(release.tag_name, installedTag)
    const beforeSelected = compare(release.tag_name, selected.tag)
    if (afterInstalled === null || afterInstalled <= 0 || beforeSelected === null || beforeSelected >= 0) continue
    seen.add(release.tag_name)
    history.push(note(release))
  }
  history.sort((left, right) => compare(left.tag, right.tag) ?? 0)
  for (let index = 1; index < history.length; index += 1) {
    const order = compare(history[index - 1].tag, history[index].tag)
    if (order === null || order > 0) return [selected]
  }
  return [...history, selected]
}

export function packageReleaseBody(body: string, locale: Locale): string {
  return selectReleaseNotes(body, locale, 'package')
}

export function installedModReleaseTag(repo: ModRepository, mods: readonly InstalledMod[]): string | undefined {
  const core = modsForRepository(mods, repo).filter(mod => {
    const binding = mod.repository_binding
    return !mod.is_dependency && !mod.release_companion && !mod.duplicate_cleanup_available
      && mod.status !== 'duplicate_lower_version' && mod.status !== 'not_release_asset'
      && (!binding || matchesRepositoryIdentity(repo, binding) && (repo.channel === 'all' || binding.channel === repo.channel))
  })
  if (core.length !== 1) return undefined
  const installed = localVersion(core[0])?.trim()
  const tag = core[0].repository_binding?.tag.trim()
  return installed && tag && compareVersions(tag, installed) === 0 ? tag : installed
}

export function mapReleaseNotesReview(map: MapInfo, target: ReleaseInfo, sets: readonly LevelSetInfo[]) {
  const matches = sets.filter(set => !set.error && !set.local_import && set.map_id === map.id)
  const installedTag = matches.length === 1 ? matches[0].installed_mapset_version?.trim() || undefined : undefined
  const table = map.asset_versions ?? []
  const history = map.shared_release_notes ? [] : map.cached_releases
  const compare: CompareTags = table.length ? (left, right) => {
    const leftIndex = assetVersionIndex(table, left)
    const rightIndex = assetVersionIndex(table, right)
    return leftIndex === undefined || rightIndex === undefined ? null : leftIndex - rightIndex
  } : compareVersions
  const base = table.length || history.some(release => release.tag_name === installedTag) ? installedTag : undefined
  return { installedTag, releases: packageReleaseNotes(history, target, base, compare) }
}
