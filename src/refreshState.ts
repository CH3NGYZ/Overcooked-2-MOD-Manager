import { computed, ref, type ComputedRef, type Ref } from 'vue'
import { mapWithConcurrency } from './concurrency.ts'

/**
 * 刷新状态机（MOD 仓库与地图共用同一份实现）。
 *
 * ## 为什么要抽出来
 *
 * 之前两套刷新各写一份状态：仓库侧在 `useRepositoryRefresh` 里维护
 * `refreshing / sourceRefreshing / refreshingAll / checkingBound`，地图侧在
 * `useMapSources` 里又写了一遍同样的四个记录。两页之间还能共享（都在模块作用域），
 * 但**规则**已经开始漂移：批次语义、并发上限、谁能触发谁、失败怎么累计，
 * 各自维护各自的。这种漂移不会报错，只会表现为「这个页在刷、那个页不转」
 * 「这里算成功、那里算失败」。
 *
 * 所以这里只留一份：谁要刷新，都拿 `createRefreshState()` 建一个实例，
 * 然后只管三件事 —— 用哪个回调刷一个条目、刷一个源、批次结束时算什么。
 *
 * ## 三个层级
 *
 * - **条目级** `refreshing[id]`：某个仓库 / 某张地图正在抓 Release；
 * - **源级** `refreshingSources[id]`：某个仓库源 / 地图源正在重拉文档
 *   （这一层必须先有：源文档阶段一个条目都还没开始刷，只靠条目级状态时，
 *   界面上什么都看不出来）；
 * - **批次级** `refreshingAll`（整批，会清空列表）与 `checkingBound`
 *   （检查更新，不动列表）。两者分开是因为副作用范围不同：
 *   整批刷新会让下载页整页进入加载态，检查更新不会。
 *
 * ## 结果
 *
 * 每个批次结束写一份 `outcome`：刷了多少、其中多少有新版、失败几条、几点完成。
 * 页面拿它显示「上次检查：6 个仓库 · 2 个有更新」，不必各拼一份。
 *
 * 本模块只用 `vue` 的 `ref`/`computed`，不碰组件、不碰 i18n —— 文案在
 * `refreshOutcomeText.ts`，由页面决定怎么说。
 */

/** 刷新来源：决定点亮哪个批次标志、以及结果怎么措辞。 */
export type RefreshTrigger =
  /** 刷新全部：清空列表 → 重拉源文档 → 刷全部条目。 */
  | 'full'
  /** 检查更新：只刷与本地已安装内容绑定的条目，不动列表。 */
  | 'bound'
  /** 局部刷新：单个源 / 单个分组，不动列表也不占批次标志。 */
  | 'partial'
  /** 单条刷新：一个仓库或一张图，不写入「上次检查」结果。 */
  | 'single'

/** 一次批次的结果。 */
export interface RefreshOutcome {
  trigger: RefreshTrigger
  /** 实际进入刷新队列的条目数（正在刷的、无 Release 可刷的不算）。 */
  refreshed: number
  /** 刷新后判定为「有新版本」的条目数；调用方没给统计函数时为 0。 */
  updated: number
  /** 失败文案，逐条展示。 */
  errors: string[]
  /** 完成时间（`Date.now()`），页面换算成「14:32」这类时间。 */
  finishedAt: number
}

export interface RefreshBatch<T> {
  items: readonly T[]
  /** 条目主键：用于去重、跳过正在刷的、以及结果里的 `refreshed` 计数。 */
  idOf: (item: T) => string
  /**
   * 刷一个条目，成功返回 `null`，失败返回可展示的文案。
   *
   * **条目自己的刷新标志由 worker 负责**（`setItemFlag`）：有的 worker 还要
   * 同时点亮别的状态（例如地图安装中），统一在这里代管反而会把那些语义吞掉。
   */
  worker: (item: T) => Promise<string | null>
  trigger: RefreshTrigger
  /** 批次收尾（通常是重读 Dashboard / 地图列表），完成后才算结果。 */
  finalize?: () => Promise<void>
  /** 统计「有新版」的条目数：必须在 `finalize` 之后读，否则读到的是旧数据。 */
  countUpdated?: () => number
}

export interface RefreshState {
  /** 条目级：按 id。 */
  refreshing: Ref<Record<string, boolean>>
  /**
   * 排队中（还没轮到、但已在这一批里）：按 id。
   *
   * 与 `refreshing` 分开、但界面上**同等对待**：用户点「检查更新」时，
   * 20 个仓库里只有 4 个能在飞，其余 16 个如果还显示旧结论，看起来就像
   * 「只有这四个在查」。所以卡片要不要显示「刷新中」看的是
   * 「在飞 ∪ 排队」，而按钮图标转不转只看「在飞」。
   */
  queued: Ref<Record<string, boolean>>
  /** 源级：按源 id（含父源与子源）。 */
  refreshingSources: Ref<Record<string, boolean>>
  /** 整批刷新进行中。 */
  refreshingAll: Ref<boolean>
  /**
   * 条目集合正在被清空/重建：这段空窗期里**条目级状态无法表达进度**。
   *
   * 整批刷新先清空条目（仓库源缓存 / 地图列表），再逐个填回来。清空之后
   * 条目本身就不存在了，`refreshing` / `queued` / `refreshingSources` 一个都
   * 匹配不上 —— 卡片此时只有两种错误选择：显示马上会被推翻的旧结论，或者
   * 退化成「未绑定」。所以这段窗口由这个标志统一兜住。
   *
   * **它不能等同于 `refreshingAll`**：后者覆盖整批（源文档 → 社区发现 →
   * 全部条目），要一直亮到按钮恢复可点为止；而条目一填回来，条目级状态就是
   * 准确的，这时再拿批次标志盖住所有条目，等于把「刷一个回来一个」压成
   * 「整批一起回来」—— 用户看到所有卡片一起转、一起停，正是要避免的。
   */
  entryStateUnavailable: Ref<boolean>
  /** 「检查更新」批次进行中。 */
  checkingBound: Ref<boolean>
  /** 最近一次批次结果（单条刷新不覆盖）。 */
  outcome: Ref<RefreshOutcome | null>
  /** 正在刷新的条目数（不含源文档阶段）。 */
  inFlight: ComputedRef<number>
  /** 任一刷新活动：按钮 loading / 禁用统一看它。 */
  anyRefreshing: ComputedRef<boolean>
  /** 写条目级标志（worker 自己调）。 */
  setItemFlag: (id: string, value: boolean) => void
  /** 写队列标志（批次实现自己调）。 */
  setQueuedFlag: (id: string, value: boolean) => void
  /** 写源级标志。 */
  setSourceFlag: (id: string, value: boolean) => void
  /** 开关「条目集合不可用」（批次实现在清空条目到填回之间自己调）。 */
  setEntryStateUnavailable: (value: boolean) => void
  /** 打开某个批次标志（`full`/`bound` 有意义，其余是空操作）。 */
  beginBatch: (trigger: RefreshTrigger) => void
  /** 关闭某个批次标志。 */
  endBatch: (trigger: RefreshTrigger) => void
  /** 在一个源级标志内跑一段任务，返回它收集到的失败文案。 */
  runSource: (id: string, worker: () => Promise<string[]>) => Promise<string[]>
  /** 跑一批条目：去重、跳过正在刷的、限并发、聚合并写结果。 */
  runBatch: <T>(batch: RefreshBatch<T>) => Promise<RefreshOutcome>
  /**
   * 直接写入一份结果。
   *
   * 给「跨阶段批次」用：整批刷新是「源文档 → 回退 → 社区发现 → 全部条目」，
   * 错误分散在四个阶段里，只有调用方知道怎么合并，`runBatch` 只看得到
   * 最后一段。单条结果会被忽略（同 `runBatch` 的规则）。
   */
  publishOutcome: (value: RefreshOutcome) => void
}

/** 批次级标志：只有这两种批次会占全局标志。 */
function batchFlagOf(trigger: RefreshTrigger): 'all' | 'bound' | null {
  if (trigger === 'full') return 'all'
  if (trigger === 'bound') return 'bound'
  return null
}

/**
 * 建一份刷新状态。
 *
 * @param concurrency 同时刷新的条目上限（两个域都用 4：再高会被匿名 API 限流，
 *   再低则几十个仓库要排很久）。
 */
export function createRefreshState(concurrency: number): RefreshState {
  const refreshing = ref<Record<string, boolean>>({})
  const queued = ref<Record<string, boolean>>({})
  const refreshingSources = ref<Record<string, boolean>>({})
  const refreshingAll = ref(false)
  const entryStateUnavailable = ref(false)
  const checkingBound = ref(false)
  const outcome = ref<RefreshOutcome | null>(null)

  /*
   * 两个记录都整体替换而不是就地增删：它们是 `ref` 的对象值，
   * 就地改属性不会触发依赖通知（页面上的标签就永远停在上一次的状态）。
   */
  const writeFlag = (target: Ref<Record<string, boolean>>, id: string, value: boolean) => {
    if (!id) return
    if (!!target.value[id] === value) return
    const next = { ...target.value }
    if (value) next[id] = true
    else delete next[id]
    target.value = next
  }

  const inFlight = computed(() => Object.keys(refreshing.value).length)
  const anyRefreshing = computed(() =>
    refreshingAll.value
    || checkingBound.value
    || inFlight.value > 0
    || Object.keys(queued.value).length > 0
    || Object.keys(refreshingSources.value).length > 0,
  )

  const beginBatch = (trigger: RefreshTrigger) => {
    const flag = batchFlagOf(trigger)
    if (flag === 'all') refreshingAll.value = true
    else if (flag === 'bound') checkingBound.value = true
  }

  const endBatch = (trigger: RefreshTrigger) => {
    const flag = batchFlagOf(trigger)
    if (flag === 'all') refreshingAll.value = false
    else if (flag === 'bound') checkingBound.value = false
  }

  const runSource = async (id: string, worker: () => Promise<string[]>) => {
    if (!id || refreshingSources.value[id]) return []
    writeFlag(refreshingSources, id, true)
    try {
      return await worker()
    } finally {
      writeFlag(refreshingSources, id, false)
    }
  }

  const publishOutcome = (value: RefreshOutcome) => {
    if (value.trigger === 'single') return
    outcome.value = value
  }

  const runBatch = async <T>(batch: RefreshBatch<T>): Promise<RefreshOutcome> => {
    /*
     * 去重 + 跳过正在刷新中的条目。
     *
     * 跳过而不是排队：同一仓库被两个入口同时点到时，第二次请求没有意义，
     * 而且它会把「刷新中」的标志提前关掉（后完成的那个先 finally），
     * 界面上就会出现「还在刷但不转了」。
     */
    const seen = new Set<string>()
    const queue = batch.items.filter((item) => {
      const id = batch.idOf(item)
      if (!id || seen.has(id)) return false
      seen.add(id)
      return !refreshing.value[id]
    })

    /*
     * 整批先入队：界面上「排队中」与「正在刷」都显示「刷新中」。
     * 20 个仓库只有 4 个能在飞，其余如果还挂着旧结论，用户会以为
     * 「只有这四个在查」—— 这也是管理页那群卡片要订阅队列的原因。
     */
    const queueIds = queue.map((item) => batch.idOf(item))
    for (const id of queueIds) writeFlag(queued, id, true)
    beginBatch(batch.trigger)
    try {
      const results = await mapWithConcurrency(queue, concurrency, async (item) => {
        /*
         * 轮到它了：立刻退出「排队中」。
         *
         * worker 进来第一件事就是同步置 `refreshing[id]`，两次写落在同一个
         * 同步段里，界面上不会闪出「既不在飞也不排队」的中间态。
         *
         * 这里以前写成 `if (refreshing.value[id])` —— 那一刻它**必然还没**
         * 在飞，条件永远不成立，于是排队标记要等整批 `finally` 才清。后果是
         * 每个目标都一直显示「刷新中」，直到最后一个请求回来才一起变成结论：
         * 用户看到的就是「结果一批一起回来」，而不是刷一个回来一个。
         */
        writeFlag(queued, batch.idOf(item), false)
        return batch.worker(item)
      })
      const errors = results.filter((error): error is string => !!error)
      await batch.finalize?.()
      const next: RefreshOutcome = {
        trigger: batch.trigger,
        refreshed: queue.length,
        updated: batch.countUpdated?.() ?? 0,
        errors,
        finishedAt: Date.now(),
      }
      /*
       * 单条刷新不覆盖「上次检查」结果：用户顺手点了一个仓库的刷新，
       * 不代表做过一次整体检查，拿它顶掉摘要会让人以为只查了这一个。
       */
      publishOutcome(next)
      return next
    } finally {
      /*
       * 无论正常结束还是中途抛错都要清掉这一批的队列标记：留着会让那些卡片
       * 永远停在「刷新中」，`anyRefreshing` 也会一直为真、所有刷新按钮变灰。
       * 只清自己这批的 id —— 另一个批次可能正排在后面。
       */
      for (const id of queueIds) writeFlag(queued, id, false)
      endBatch(batch.trigger)
    }
  }

  return {
    refreshing,
    queued,
    refreshingSources,
    refreshingAll,
    entryStateUnavailable,
    checkingBound,
    outcome,
    inFlight,
    anyRefreshing,
    setItemFlag: (id, value) => writeFlag(refreshing, id, value),
    setQueuedFlag: (id, value) => writeFlag(queued, id, value),
    setSourceFlag: (id, value) => writeFlag(refreshingSources, id, value),
    setEntryStateUnavailable: (value) => { entryStateUnavailable.value = value },
    beginBatch,
    endBatch,
    runSource,
    runBatch,
    publishOutcome,
  }
}
