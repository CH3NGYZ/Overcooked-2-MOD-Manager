import type { RefreshOutcome, RefreshState, RefreshTrigger } from './refreshState'

/**
 * 「一批带 Release 的条目」怎么刷 —— MOD 仓库与地图条目共用这一份实现。
 *
 * ## 为什么两个域能共用
 *
 * 它们本质上是同一种东西：GitHub / Gitee 上的一个仓库，要拉的是它的 Release
 * 列表，结论是「有没有比本地新的版本」。后端虽然给了两条命令
 * （`refresh_repository` / `refresh_map_releases`），但「刷一个条目」和
 * 「刷一批条目」的形状完全一致：点亮标志 → 请求 → 写回缓存 → 关标志 →
 * 整批结束后清点失败与「有新版」的数量。
 *
 * 之前这两段各写一份，差别只在「把结果写回哪里」和「失败文案怎么拼」。
 * 结果就是同一个 4 并发上限、同一套「跳过正在刷的」规则维护了两遍，
 * 改一处忘一处时表现为两边的刷新节奏对不上。
 *
 * ## 什么不在这里
 *
 * **源文档层**（map: `refreshMapSources` 一次性重拉整棵树；repo: 逐源 TOML
 * + 递归子源 + 内置快照回退 + 社区发现）。那部分后端的命令与语义本来就不同，
 * 硬合并会得到一个充满 `if (domain === ...)` 的伪通用层；它们各自在域适配器里，
 * 但共用同一个状态机的源级标志（`runSource`）。
 */

/** 能被刷新的条目：只要有稳定 id，长度/版本由调用方自己拿。 */
export interface ReleaseItem {
  id: string
}

export interface ReleaseBatchOptions<T extends ReleaseItem> {
  /** 共享状态机（两个域各一个实例，同一种实现）。 */
  state: RefreshState
  items: readonly T[]
  /**
   * 拉这一条的最新 Release 并写回缓存；成功返回 `null`，失败返回可展示文案。
   *
   * 错误文案由调用方拼：仓库要说清「哪个仓库失败」（`<仓库名>: <原因>`），
   * 地图那边文案里已经带了标题，强行统一反而会少信息。
   */
  fetch: (item: T) => Promise<string | null>
  trigger: RefreshTrigger
  /** 已安装优先排序：用户装过的条目排到队首（并发 4，排前面才先出结论）。 */
  prioritize?: (items: readonly T[]) => T[]
  /** 批次收尾（重读 Dashboard / 地图列表）；它跑完才开始统计「有新版」。 */
  finalize?: () => Promise<void>
  /** 统计「有新版」的条目数；必须在 `finalize` 之后读，否则是上一轮结论。 */
  countUpdated?: () => number
}

/**
 * 刷一个条目：点亮「刷新中」→ 拉 Release → 关标志。
 *
 * 标志一定要在 `finally` 里关：请求抛错时留在「刷新中」会让按钮永远转下去。
 * 已经在刷的直接返回 `null`（当作成功但不重复发请求）——
 * 两个页面同时点同一个仓库时，第二下不该产生第二个请求。
 */
export async function refreshReleaseItem<T extends ReleaseItem>(
  state: RefreshState,
  item: T,
  fetch: () => Promise<string | null>,
): Promise<string | null> {
  if (!item.id || state.refreshing.value[item.id]) return null
  state.setItemFlag(item.id, true)
  try {
    return await fetch()
  } finally {
    state.setItemFlag(item.id, false)
  }
}

/**
 * 把一批里「用户装过的」排到队首，其余保持原顺序。
 *
 * 地图侧没有「安装清单」这样的统一入口（本地关卡集与地图条目是两套身份），
 * 所以由页面把绑定上的地图 id 传进来；MOD 侧直接复用
 * `repositoryPriority.prioritizeInstalledRepositories`。
 */
export function prioritizeByIds<T extends ReleaseItem>(
  items: readonly T[],
  ids: ReadonlySet<string>,
): T[] {
  if (!ids.size) return [...items]
  const first: T[] = []
  const rest: T[] = []
  for (const item of items) {
    if (ids.has(item.id)) first.push(item)
    else rest.push(item)
  }
  return [...first, ...rest]
}

/**
 * 刷一批条目：已安装优先 → 去重 → 4 并发 → 聚合并写结果。
 *
 * 去重、跳过正在刷的、并发上限、结果统计全部由状态机负责；
 * 这里只把「域的顺序偏好」和「一个条目怎么刷」交进去。
 */
export function runReleaseBatch<T extends ReleaseItem>(
  options: ReleaseBatchOptions<T>,
): Promise<RefreshOutcome> {
  const items = options.prioritize ? options.prioritize(options.items) : options.items
  return options.state.runBatch({
    items,
    idOf: (item) => item.id,
    worker: (item) => options.fetch(item),
    trigger: options.trigger,
    finalize: options.finalize,
    countUpdated: options.countUpdated,
  })
}
