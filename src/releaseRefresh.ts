import type { RefreshOutcome, RefreshState, RefreshTrigger } from './refreshState'
import type { ReleaseInfo } from './types'

/**
 * 「刷一个带 Release 的条目」与「刷一批」—— MOD 仓库与地图条目共用这一份实现。
 *
 * ## 为什么两个域能共用
 *
 * 它们本质上是同一种东西：GitHub / Gitee 上的一个仓库，要拉的是它的 Release
 * 列表，结论是「有没有比本地新的版本」。后端虽然给了两条命令
 * （`refresh_repository` / `refresh_map_releases`），但形状完全一致：
 * 请求 → 成功后写回这个条目的缓存 → 失败后把这个条目标成出错 → 无论哪种
 * 都关掉它自己的「刷新中」标志。
 *
 * 之前这两段各写一份，差别只在「把结果写回哪里」和「失败文案怎么拼」。
 * 结果就是同一个 4 并发上限、同一套「跳过正在刷的」规则维护了两遍，
 * 改一处忘一处时表现为两边的刷新节奏对不上。
 *
 * ## 「刷一个回来一个」由结构保证
 *
 * 单个条目一旦拿到自己的结果就**当场**写回并清掉自己的标志
 * （`refreshReleaseItem`），批次只是把它们按并发上限排队 —— 没有任何一步
 * 需要等别的条目。失败同样当场落地：成功后写缓存、失败后写 `last_error`，
 * 两者都在关标志之前完成，卡片不会先变成「不是刷新中、结论还是旧的」，
 * 再补一次更新。
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

/**
 * 「刷一个条目」要做的三件事：怎么请求、成功写回哪里、失败写回哪里。
 *
 * 两个域只提供这三个回调，其余（标志、并发、结果收集）都在这里。
 */
export interface ReleaseFetchSpec<T extends ReleaseItem> {
  /** 请求这一条的最新 Release；抛错即按失败处理（两个 IPC 命令本身就是抛错的）。 */
  fetch: (item: T) => Promise<ReleaseInfo[]>
  /**
   * 成功写回：把结果落到这个条目自己的缓存上。
   *
   * 必须只改这一个条目（其余条目沿用原对象）：整批刷新期间每个条目都会
   * 走一遍这里，整体替换快照会让所有卡片跟着抖一次。
   */
  apply?: (item: T, releases: ReleaseInfo[]) => void
  /**
   * 失败写回：把这个条目标成出错，让卡片立刻显示「刷新失败」。
   *
   * 不写的话，同一批里成功的条目一个个变，失败的会一直挂着上一轮的
   * 「最新版」，直到整批结束、后端重扫才一起变 —— 又变成「一批一起回来」。
   */
  fail?: (item: T, message: string) => void
  /**
   * 失败文案（`<仓库名>: <原因>`）。
   *
   * 由域自己拼：仓库要说清「哪个仓库失败」，地图那边的文案里已经带了标题，
   * 强行统一反而会少信息。
   */
  errorText: (item: T, reason: unknown) => string
}

export interface ReleaseBatchOptions<T extends ReleaseItem> extends ReleaseFetchSpec<T> {
  /** 共享状态机（两个域各一个实例，同一种实现）。 */
  state: RefreshState
  items: readonly T[]
  trigger: RefreshTrigger
  /** 已安装优先排序：用户装过的条目排到队首（并发 4，排前面才先出结论）。 */
  prioritize?: (items: readonly T[]) => T[]
  /** 批次收尾（重读 Dashboard / 地图列表）；它跑完才开始统计「有新版」。 */
  finalize?: () => Promise<void>
  /** 统计「有新版」的条目数；必须在 `finalize` 之后读，否则是上一轮结论。 */
  countUpdated?: () => number
}

/**
 * 刷一个条目：点亮「刷新中」→ 请求 → 写回 → 关标志。
 *
 * 标志一定要在 `finally` 里关：请求抛错时留在「刷新中」会让按钮永远转下去。
 * 已经在刷的直接返回 `null`（当作成功但不重复发请求）——
 * 两个页面同时点同一个仓库时，第二下不该产生第二个请求。
 */
export async function refreshReleaseItem<T extends ReleaseItem>(
  state: RefreshState,
  item: T,
  spec: ReleaseFetchSpec<T>,
): Promise<string | null> {
  if (!item.id || state.refreshing.value[item.id]) return null
  state.setItemFlag(item.id, true)
  try {
    const releases = await spec.fetch(item)
    /*
     * 写回必须早于关标志：反过来会先出现一帧「不在刷新中、但结论还是旧的」，
     * 然后才被这次的结果替换 —— 看起来像闪了一下旧状态。
     */
    spec.apply?.(item, releases)
    return null
  } catch (reason) {
    const message = spec.errorText(item, reason)
    spec.fail?.(item, message)
    return message
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
 * 刷一批条目：已安装优先 → 去重 → 4 并发 → 逐条落地 → 聚合并写结果。
 *
 * 每个条目走的就是 `refreshReleaseItem` —— 与用户单独点某一条的刷新是
 * 同一份逻辑，批次**不**参与单条结果的落地，只负责排队与最后的统计。
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
    worker: (item) => refreshReleaseItem(options.state, item, options),
    trigger: options.trigger,
    finalize: options.finalize,
    countUpdated: options.countUpdated,
  })
}
