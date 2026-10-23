// Sort tiers for the installed MOD list. Lower numbers are more urgent:
//   0  checking / pending (transient, already being resolved)
//   1  update available, enabled
//   2  update available but disabled — still needs the user's attention
//   3  latest, enabled
//   4  repository-file (Raw) assets, updates cannot be checked
//   5  no repository bound
//   6  refresh failed / version information insufficient
//   7  cleanup warnings while enabled (duplicates, higher local version)
//   8  latest but disabled, or disabled without another state
//   9  duplicate and disabled
//  10  a higher local version exists and the MOD is disabled
const TIER = {
  checking: 0,
  updateEnabled: 1,
  updateDisabled: 2,
  latestEnabled: 3,
  rawAsset: 4,
  unbound: 5,
  unknown: 6,
  cleanupEnabled: 7,
  latestDisabled: 8,
  duplicateDisabled: 9,
  lowerVersionDisabled: 10,
} as const

const LEGACY_CODES: Record<string, string> = {
  '检查中': 'checking',
  '待检查': 'pending',
  '有更新': 'update_available',
  '最新版': 'latest',
  '不是 Release 资产, 无法检测更新': 'not_release_asset',
  '未绑定在线仓库，无法检测更新': 'unbound',
  '已禁用': 'disabled',
}

// Badge order inside one MOD row and its package heading.
export const installedModStatusOrder: readonly string[] = [
  'checking', 'pending', 'update_available', 'latest', 'not_release_asset', 'unbound',
  'bound_repository_missing', 'repository_unavailable', 'unknown', 'duplicate_lower_version',
  'duplicate', 'load_warning', 'disabled',
]

function statusCodes(status: string | readonly string[]): Set<string> {
  const list = Array.isArray(status) ? status : [status]
  const codes = new Set<string>()
  for (const item of list) codes.add(LEGACY_CODES[item as string] ?? (item as string))
  return codes
}

export function installedModStatusPriority(status: string | readonly string[]): number {
  const codes = statusCodes(status)
  if (codes.has('checking') || codes.has('pending')) return TIER.checking
  const disabled = codes.has('disabled')
  if (codes.has('update_available')) return disabled ? TIER.updateDisabled : TIER.updateEnabled
  if (disabled) {
    if (codes.has('duplicate_lower_version') || codes.has('load_warning')) return TIER.lowerVersionDisabled
    if (codes.has('duplicate')) return TIER.duplicateDisabled
    return TIER.latestDisabled
  }
  if (codes.has('latest')) return TIER.latestEnabled
  if (codes.has('not_release_asset')) return TIER.rawAsset
  if (codes.has('unbound') || codes.has('bound_repository_missing')) return TIER.unbound
  if (codes.has('duplicate_lower_version') || codes.has('duplicate') || codes.has('load_warning')) {
    return TIER.cleanupEnabled
  }
  return TIER.unknown
}

type ModGroup<T> = {
  id: string
  name: string
  priority: number
  mods: T[]
}

export function sortInstalledModGroups<T>(
  mods: readonly T[],
  getPackageId: (mod: T) => string | undefined,
  getStatus: (mod: T) => string | readonly string[],
  getGroupName: (mod: T, packageId?: string) => string,
  getName: (mod: T) => string,
  getFileName: (mod: T) => string,
  compareText: (left: string, right: string) => number,
): T[] {
  const groups = new Map<string, ModGroup<T>>()

  for (const mod of mods) {
    const packageId = getPackageId(mod)
    const id = packageId ? `package:${packageId}` : `mod:${getFileName(mod)}`
    const priority = installedModStatusPriority(getStatus(mod))
    const group = groups.get(id)
    if (group) {
      group.mods.push(mod)
      group.priority = Math.min(group.priority, priority)
    } else {
      groups.set(id, {
        id,
        name: getGroupName(mod, packageId),
        priority,
        mods: [mod],
      })
    }
  }

  return [...groups.values()]
    .sort((left, right) => left.priority - right.priority
      || compareText(left.name, right.name)
      || compareText(left.id, right.id))
    .flatMap((group) => group.mods
      .sort((left, right) => compareText(getName(left), getName(right))
        || compareText(getFileName(left), getFileName(right))))
}
