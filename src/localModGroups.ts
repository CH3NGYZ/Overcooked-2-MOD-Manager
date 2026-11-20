import { compareVersions, repositoryForMod } from './modStatus.ts'
import {
  installedModTags,
  packageStatuses,
  summarizeUnits,
  type PackageStatus,
  type StatusSummary,
  type StatusUnit,
} from './statusTags.ts'
import type { InstalledMod, ModRepository } from './types'

export interface ModPackageNode {
  key: string
  label: string
  mods: InstalledMod[]
  visibleMods: InstalledMod[]
  /** Keep the core details visible even when the search found a bundled DLL. */
  coreMods: InstalledMod[]
  grouped: boolean
  canRemovePackage: boolean
  version?: string
  enabled: boolean
  partial: boolean
  statuses: PackageStatus[]
  repositoryId?: string
}

export interface ModGroupNode {
  key: string
  label: string
  packages: ModPackageNode[]
}

export interface LocalModCategory {
  key: string
  label: string
}

export interface LocalModCategoryTexts {
  ungrouped: string
  localInstall: string
  unbound: string
}

export interface LocalModGroupOptions {
  mods: readonly InstalledMod[]
  repositories: ModRepository[]
  matchedDllFiles: ReadonlySet<string> | null
  categoryTexts: LocalModCategoryTexts
  pickI18n(map: Record<string, string> | undefined, fallback: string): string
  displayTitle(mod: InstalledMod): string
}

export interface LocalModStatusUnit {
  keys: string[]
  packageKey: string
}

export interface LocalModStatusSummary {
  all: StatusSummary[]
  primary: StatusSummary[]
  toggle: StatusSummary[]
}

const TOGGLE_STATUS_KEYS = ['enabled', 'disabled'] as const
const TOGGLE_STATUS_KEY_SET: ReadonlySet<string> = new Set(TOGGLE_STATUS_KEYS)
const TAIL_GROUPS = new Set(['__unbound__', '__ungrouped__'])

/** Show the installed release when it agrees with the identified core DLL. */
function localPackageVersion(members: readonly InstalledMod[]): string | undefined {
  const installedPackage = members.filter((mod) => !mod.bundled_with_dll)
  const plugins = installedPackage.filter((mod) => !mod.is_dependency)
  const guidMatches = plugins.filter((mod) => mod.repository_identity_match === 'guid')
  const metadataMatches = plugins.filter((mod) => mod.repository_identity_match === 'metadata')
  // Every DLL in a manifest shares the tag. A missing identity isn't evidence
  // that its remaining member is the core (the real core may be missing).
  const tags = new Set(installedPackage.map((mod) => mod.repository_binding?.tag.trim()).filter(Boolean))
  const release = tags.size === 1 ? tags.values().next().value : undefined
  const candidates = guidMatches.length ? guidMatches : metadataMatches.length ? metadataMatches : tags.size ? [] : plugins
  const actual = candidates.length === 1 ? candidates[0].version?.trim() : undefined
  if (!actual) return release
  return release && compareVersions(release, actual) === 0 ? release : actual
}

/** Package identity is stable when search or status filtering changes. */
export function packageKeyOf(mod: InstalledMod): string {
  return mod.package_id ? `pkg:${mod.package_id}` : `dll:${mod.dll_file}`
}

/** Group a local MOD using its install-time category snapshot and current repository. */
export function localModCategory(
  mod: InstalledMod,
  repo: ModRepository | undefined,
  texts: LocalModCategoryTexts,
  pickI18n: LocalModGroupOptions['pickI18n'],
): LocalModCategory {
  const snapshot = mod.group_i18n
  const hasSnapshot = !!snapshot && Object.keys(snapshot).length > 0
  if (repo) {
    if (hasSnapshot) {
      const canonical = (snapshot['zh-CN'] || pickI18n(snapshot, '')).trim()
      const label = pickI18n(snapshot, '') || texts.ungrouped
      return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
    }
    const canonical = (repo.group || repo.group_i18n?.['zh-CN'] || pickI18n(repo.group_i18n, '')).trim()
    const label = pickI18n(repo.group_i18n, repo.group || '') || texts.ungrouped
    return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
  }
  if (mod.manual_install) return { key: '__local_install__', label: texts.localInstall }
  if (hasSnapshot) {
    const canonical = (snapshot['zh-CN'] || pickI18n(snapshot, '')).trim()
    const label = pickI18n(snapshot, '') || texts.ungrouped
    return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
  }
  return { key: '__unbound__', label: texts.unbound }
}

/**
 * Project sorted installed MODs into group/package cards.
 * Search narrows visible members only; package operations retain the full member list.
 */
export function buildLocalModGroups(options: LocalModGroupOptions): ModGroupNode[] {
  const { mods, repositories, matchedDllFiles, categoryTexts, pickI18n, displayTitle } = options
  const byDll = new Map(mods.map((mod) => [mod.dll_file, mod]))
  const anchors = new Set<string>()
  const packagesByKey = new Map<string, InstalledMod[]>()
  for (const mod of mods) {
    const candidate = !mod.package_id && mod.bundled_with_dll ? byDll.get(mod.bundled_with_dll) : undefined
    const core = candidate && candidate !== mod && !candidate.bundled_with_dll ? candidate : undefined
    if (core) anchors.add(core.dll_file)
    const key = packageKeyOf(core ?? mod)
    const members = packagesByKey.get(key)
    if (members) members.push(mod)
    else packagesByKey.set(key, [mod])
  }

  const representative = (members: readonly InstalledMod[]): InstalledMod =>
    members.find((mod) => anchors.has(mod.dll_file))
      ?? members.find((mod) => repositoryForMod(repositories, mod))
      ?? members.find((mod) => !mod.is_dependency)
      ?? members[0]

  const buckets = new Map<string, { label: string; packages: Map<string, InstalledMod[]> }>()
  for (const [key, members] of packagesByKey) {
    const category = localModCategory(
      representative(members),
      repositoryForMod(repositories, representative(members)),
      categoryTexts,
      pickI18n,
    )
    let bucket = buckets.get(category.key)
    if (!bucket) {
      bucket = { label: category.label, packages: new Map() }
      buckets.set(category.key, bucket)
    }
    bucket.packages.set(key, members)
  }

  const toPackage = (key: string, members: InstalledMod[]): ModPackageNode => {
    const ordered = [...members].sort(
      (left, right) => Number(anchors.has(right.dll_file)) - Number(anchors.has(left.dll_file))
        || Number(!!left.is_dependency) - Number(!!right.is_dependency),
    )
    const enabledCount = ordered.filter((mod) => mod.enabled).length
    const statuses = packageStatuses(ordered)
    const packageRepresentative = representative(ordered)
    const repo = repositoryForMod(repositories, packageRepresentative)
    return {
      key,
      label: ordered[0] ? displayTitle(ordered[0]) : '',
      mods: ordered,
      visibleMods: ordered.filter((mod) => matchedDllFiles === null || matchedDllFiles.has(mod.dll_file)),
      coreMods: ordered.filter(mod => !mod.is_dependency && !mod.bundled_with_dll && !mod.release_companion),
      grouped: ordered.length >= 2,
      canRemovePackage: ordered.every((mod) => !!mod.package_id && mod.package_id === ordered[0]?.package_id),
      version: ordered.length >= 2 ? localPackageVersion(ordered) : undefined,
      enabled: enabledCount === ordered.length,
      partial: enabledCount > 0 && enabledCount < ordered.length,
      statuses,
      repositoryId: repo?.id,
    }
  }

  return [...buckets.entries()]
    .sort(([leftKey, left], [rightKey, right]) => {
      if (TAIL_GROUPS.has(leftKey) !== TAIL_GROUPS.has(rightKey))
        return TAIL_GROUPS.has(leftKey) ? 1 : -1
      return left.label.localeCompare(right.label)
    })
    .map(([key, bucket]) => ({
      key,
      label: bucket.label,
      packages: [...bucket.packages.entries()]
        .map(([packageKey, members]) => toPackage(packageKey, members))
        .filter((pkg) => pkg.visibleMods.length > 0),
    }))
    .filter((group) => group.packages.length > 0)
}

/** Status aggregation and status filtering share the card's public unit semantics. */
export function localModStatusUnits(groups: readonly ModGroupNode[]): LocalModStatusUnit[] {
  const units: LocalModStatusUnit[] = []
  for (const group of groups) {
    for (const pkg of group.packages) {
      if (pkg.grouped) {
        const enabled = pkg.mods.every((mod) => mod.enabled)
        const disabled = pkg.mods.every((mod) => !mod.enabled)
        units.push({
          keys: [
            ...pkg.statuses.map((status) => status.key),
            ...(enabled ? ['enabled'] : disabled ? ['disabled'] : []),
          ],
          packageKey: pkg.key,
        })
        continue
      }
      for (const mod of pkg.mods) {
        if (mod.is_dependency) continue
        units.push({
          keys: [
            ...installedModTags(mod).map((tag) => tag.key),
            ...(mod.enabled ? ['enabled'] : ['disabled']),
          ],
          packageKey: pkg.key,
        })
      }
    }
  }
  return units
}

export function summarizeLocalModStatuses(
  units: readonly LocalModStatusUnit[],
): LocalModStatusSummary {
  const all = summarizeUnits(units.map((unit): StatusUnit => ({ keys: unit.keys })))
  return {
    all,
    primary: all.filter((entry) => !TOGGLE_STATUS_KEY_SET.has(entry.key)),
    toggle: TOGGLE_STATUS_KEYS
      .map((key) => all.find((entry) => entry.key === key))
      .filter((entry): entry is StatusSummary => !!entry),
  }
}

export function packageKeysMatchingStatus(
  units: readonly LocalModStatusUnit[],
  statusMatches: (keys: readonly string[]) => boolean,
): Set<string> {
  return new Set(units.filter((unit) => statusMatches(unit.keys)).map((unit) => unit.packageKey))
}

export function filterLocalModGroups(
  groups: readonly ModGroupNode[],
  packageKeys: ReadonlySet<string>,
): ModGroupNode[] {
  return groups
    .map((group) => ({
      ...group,
      packages: group.packages.filter((pkg) => packageKeys.has(pkg.key)),
    }))
    .filter((group) => group.packages.length > 0)
}
