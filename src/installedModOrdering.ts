const STATUS_PRIORITY: Record<string, number> = {
  update_available: 0,
  '有更新': 0,
  latest: 1,
  '最新版': 1,
  not_release_asset: 2,
  '不是 Release 资产, 无法检测更新': 2,
  unbound: 3,
  '未绑定在线仓库，无法检测更新': 3,
  disabled: 5,
  '已禁用': 5,
}

export function installedModStatusPriority(status: string | readonly string[]): number {
  if (Array.isArray(status)) return status.length ? status.reduce((priority, code) => Math.min(priority, STATUS_PRIORITY[code] ?? 4), 5) : 4
  return STATUS_PRIORITY[status as string] ?? 4
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
