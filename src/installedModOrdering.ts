// Checking states sort above settled results: a MOD that is being checked is
// shown first, then update/latest and the remaining states.
const UNKNOWN_PRIORITY = 5

const STATUS_PRIORITY: Record<string, number> = {
  checking: 0,
  '检查中': 0,
  pending: 0,
  '待检查': 0,
  update_available: 1,
  '有更新': 1,
  latest: 2,
  '最新版': 2,
  not_release_asset: 3,
  '不是 Release 资产, 无法检测更新': 3,
  unbound: 4,
  '未绑定在线仓库，无法检测更新': 4,
  disabled: 6,
  '已禁用': 6,
}

// Badge order inside one MOD row and its package heading.
export const installedModStatusOrder: readonly string[] = [
  'checking', 'pending', 'update_available', 'latest', 'not_release_asset', 'unbound',
  'bound_repository_missing', 'repository_unavailable', 'unknown', 'duplicate_lower_version',
  'duplicate', 'load_warning', 'disabled',
]

export function installedModStatusPriority(status: string | readonly string[]): number {
  if (Array.isArray(status)) {
    return status.length
      ? status.reduce((priority, code) => Math.min(priority, STATUS_PRIORITY[code] ?? UNKNOWN_PRIORITY), 6)
      : UNKNOWN_PRIORITY
  }
  return STATUS_PRIORITY[status as string] ?? UNKNOWN_PRIORITY
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
