/**
 * 地图源数据与刷新引擎（**模块级单例**）。
 *
 * ## 为什么不是页面内的 `ref`
 *
 * 整源刷新要依次拉「索引 → 子源 → 每张图的 Release」，真实网络下好几秒。
 * 用户在这期间切走再切回来时，页面实例已经卸载重建 —— 状态挂在页面上的话
 * 就全丢了：回来只剩一个旧列表，用户完全不知道后台那次刷新还在跑，
 * 也看不到任何加载反馈。
 *
 * MOD 安装页没有这个问题，因为它的刷新状态在 `useRepositoryRefresh` 里同样
 * 是模块级单例。这里与之对齐：数据与状态都在模块作用域，页面只是渲染它们。
 *
 * ## 与 `useRepositoryRefresh` 的差别
 *
 * MOD 侧的数据源是 Dashboard（`game`/`mods`/`config`），刷新引擎只持有
 * 「谁在刷」。地图侧的数据是独立的一份（`maps`），没有对应的 store，
 * 所以数据和引擎一起放在这里。
 */
import { ref } from 'vue'
import { api } from '../services/tauri'
import { createRefreshState, type RefreshOutcome, type RefreshTrigger } from '../refreshState'
import { prioritizeByIds, runReleaseBatch, type ReleaseFetchSpec } from '../releaseRefresh'
import { backendMessageOf } from '../locale/backend'
import { useNotify, type Notify } from '../useNotify'
import type { MapInfo } from '../types'

/** 并行上限（4 组），与仓库侧 `useRepositoryRefresh` 的取值保持一致。 */
const REFRESH_CONCURRENCY = 4

/** 地图源声明的全部地图（含 Release 缓存）。 */
const maps = ref<MapInfo[]>([])
/** 首次读数据中（只影响整页加载态，不影响刷新）。 */
const loading = ref(true)
/** 每张地图的安装状态。安装进行中切走再回来也要保持。 */
const installing = ref<Record<string, boolean>>({})

/**
 * 刷新状态（与仓库侧 `useRepositoryRefresh` 同一份实现 `createRefreshState`）。
 *
 * 条目级 `refreshing`、源级 `refreshingSources`、整批 `refreshingAll`、
 * 检查更新 `checkingBound` 与结果 `outcome` 全部来自它 —— 两套刷新的**规则**
 * 只有一处，不会一边按 4 并发、另一边按串行，也不会一边算成功、另一边算失败。
 */
const state = createRefreshState(REFRESH_CONCURRENCY)
/** 每张地图的 Release 刷新状态。 */
const refreshing = state.refreshing
/** 排队中（已在这一批里、还没轮到）：界面上与「刷新中」同等对待。 */
const queued = state.queued
/** 每个地图源的刷新状态（源树重拉 + 该源下全部 Release）。 */
const refreshingSources = state.refreshingSources
/** 整源刷新进行中（浮动按钮的转圈与整页遮罩都用它）。 */
const refreshingAll = state.refreshingAll
/**
 * 「检查更新」批次进行中（只刷新本地已装地图绑定的那几张）。
 *
 * 与 `refreshingAll` 分开：整源刷新会清空列表并让下载页整页进入加载态，
 * 检查更新不动源树、不清列表。两个页面读同一个标志，
 * 所以从地图管理页点「检查更新」时，下载页的刷新按钮也会跟着禁用。
 */
const checkingBound = state.checkingBound
/** 最近一次批次结果（地图管理页的「上次检查」摘要读它）。 */
const outcome = state.outcome

/** 任一刷新活动：两个页面的按钮 loading / 禁用统一看它。 */
const anyRefreshing = state.anyRefreshing

/**
 * 通知入口。
 *
 * `useNotify()` 依赖 `n-message-provider` 的注入，必须在组件 setup 里取，
 * 所以这里延后到第一次调用 `useMapSources()` 时初始化；之后复用同一个实例
 * （它的底层是全局 provider，不随发起组件销毁而失效）。
 */
let message: Notify | null = null

/** 写条目级标志（安装流程也会用它标记「正在安装」之外的刷新态）。 */
function setFlag(flags: typeof refreshing, id: string, value: boolean) {
  const next = { ...flags.value }
  if (value) next[id] = true
  else delete next[id]
  flags.value = next
}

/**
 * 读一次地图列表（后端缓存，只读不联网）。
 *
 * `force` 区分两种调用方：
 *   · 页面挂载时读 —— 整源刷新进行中就**跳过**。否则切走再切回来时，这里会用
 *     旧缓存把刚清空的列表填回去，遮罩随之提前消失，用户看到的是「还在刷新
 *     但看起来已经好了」。
 *   · 刷新流程自己收尾时读 —— 必须强制，那时列表正等着换成新数据。
 */
async function loadMaps(options: { force?: boolean } = {}) {
  if (refreshingAll.value && !options.force) return
  loading.value = maps.value.length === 0
  try {
    maps.value = await api.maps()
  } catch (reason) {
    message?.error(backendMessageOf(reason))
  } finally {
    loading.value = false
  }
}

async function syncDependencySnapshots() {
  await api.syncDependencySnapshots().catch(() => undefined)
}

/**
 * 逐个刷新一批地图的 Release，返回失败信息。
 *
 * 单个失败不阻断同批其它条目：一次刷几十张图时逐条弹错会把用户淹没，所以
 * 错误交回调用方合并上报（与 MOD 安装页的批量刷新同一套做法）。
 *
 * 并发 4 组：原来是纯串行，7 张图要排 7 个来回；改成与仓库侧同一个上限后
 * 检查更新明显变快，而 4 个并发也远达不到代理/API 的限流阈值。
 *
 * 结束后把线上依赖声明写回本地清单并重读列表：卡片状态与依赖表要立刻反映
 * 新缓存，否则用户刷完看到还是旧版本。
 */
/**
 * 地图域「刷一个条目」的全部回调：单条刷新与批量刷新共用这一份定义。
 *
 * 与仓库侧的 `repositoryReleaseSpec` 同构 —— 差别只有「请求哪个命令」
 * 和「写回哪个对象」，其余（标志、写回时机、并发、结果收集）都在
 * `releaseRefresh` / `refreshState` 里。
 */
function mapReleaseSpec(): ReleaseFetchSpec<MapInfo> {
  return {
    // 用户主动刷新 → 强制绕过 5 分钟 Release 缓存
    fetch: (item) => api.refreshMapReleases(item.id, true),
    /*
     * 成功写回只替换这一张图，不等整批结束重载列表：`latest_tag` 取自缓存的
     * 第一条，与后端 `map_to_info` 完全同一条规则（所以不必在前端另立判定）。
     */
    apply: (item, releases) => patchMap(item.id, {
      cached_releases: releases,
      latest_release: releases[0] ?? null,
      latest_tag: releases[0]?.tag_name ?? null,
      cached_release_count: releases.length,
      cached_at: Date.now(),
      last_error: null,
    }),
    /* 与后端 Err 分支同一规则：清缓存 + 置错，卡片当场显示「刷新失败」 */
    fail: (item, message) => patchMap(item.id, {
      cached_releases: [],
      latest_release: null,
      latest_tag: null,
      cached_release_count: 0,
      cached_at: null,
      last_error: message,
    }),
    errorText: (_item, reason) => backendMessageOf(reason),
  }
}

/**
 * 只替换列表里的一行（不动其它地图的引用）。
 *
 * 地图没有 Dashboard 那样的外部 store，`maps` 就是唯一的真源，
 * 所以「刷一张回来一张」必须在这里就地落地；整批结束的 `loadMaps()` 只负责
 * 补齐依赖快照这类派生数据。
 */
function patchMap(mapId: string, patch: Partial<MapInfo>) {
  const index = maps.value.findIndex((map) => map.id === mapId)
  if (index < 0) return
  const next = maps.value.slice()
  next[index] = { ...next[index], ...patch }
  maps.value = next
}

async function refreshReleases(
  list: MapInfo[],
  options: {
    trigger?: RefreshTrigger
    /** 已装地图的 id：排到队首，先出结论。 */
    priorityIds?: ReadonlySet<string>
    countUpdated?: () => number
  } = {},
): Promise<RefreshOutcome> {
  const priorityIds = options.priorityIds
  return runReleaseBatch<MapInfo>({
    state,
    // 正在安装的不能同时刷：安装要替换目录，刷新会读它的元数据
    items: list.filter((map) => !installing.value[map.id]),
    ...mapReleaseSpec(),
    trigger: options.trigger ?? 'partial',
    prioritize: priorityIds && priorityIds.size
      ? (items) => prioritizeByIds(items, priorityIds)
      : undefined,
    finalize: async () => {
      await syncDependencySnapshots()
      await loadMaps({ force: true })
    },
    countUpdated: options.countUpdated,
  })
}

/** 一次「检查更新」的结果：刷新了多少张图、其中多少有新版、哪些失败。 */
export interface MapBoundRefreshSummary {
  /** 实际进入刷新队列的地图数（已跳过正在刷新/安装中的）。 */
  refreshed: number
  /** 刷新后判定为「有新版」的关卡集数。 */
  updated: number
  /** 失败文案，调用方逐条提示。 */
  errors: string[]
}

/**
 * 检查更新：只刷新「本地已装关卡集绑定」的那几张地图，**强制**绕过缓存。
 *
 * ## 为什么由引擎暴露
 *
 * 地图管理页原来自己 `for` 循环调 `api.refreshMapReleases`，状态挂在页面
 * 局部的 `checkingUpdates` 上 —— 于是同一件事在两页之间完全没有交汇：
 * 管理页在刷、下载页的卡片不转，切走再回来按钮还会『假装没在刷』。
 * 现在把入口收到这里，两个页面共用同一份 `refreshing` / `checkingBound`。
 *
 * ## 为什么不重拉源树
 *
 * 源树重拉会先清空该源下的地图条目再重建，缓存一起丢；检查更新只想知道
 * 「这几张图有没有新版」，不该顺手把列表洗一遍。要源数据就点整源刷新。
 */
async function refreshBoundMaps(
  targets: MapInfo[],
  options: { countUpdated?: () => number } = {},
): Promise<MapBoundRefreshSummary> {
  /*
   * 去重交给 `runBatch`（它按 id 去重并跳过正在刷的），这里只筛掉安装中的：
   * 同一张图被两个页面同时点「检查更新」时，第二份请求没有意义。
   */
  const queue = targets.filter((map) => !installing.value[map.id])
  const result = await refreshReleases(queue, { trigger: 'bound', ...options })
  return { refreshed: result.refreshed, updated: result.updated, errors: result.errors }
}

/**
 * 刷新一个源。
 *
 * 后端没有「只刷新一个源」的命令：地图源树的加载本身就是一次整体请求，
 * 所以这里先重拉源树（可能新增/删除地图、改分组），再刷新这个源下的
 * 全部 Release。重拉后按源 ID 重新取一次条目，不吃重拉前的旧对象。
 */
async function refreshSource(node: { id: string }) {
  if (refreshingAll.value || refreshingSources.value[node.id]) return
  await state.runSource(node.id, async () => {
    try {
      maps.value = await api.refreshMapSources()
    } catch (reason) {
      message?.error(backendMessageOf(reason))
    }
    const scoped = maps.value.filter((map) => (map.source_id || '__none__') === node.id)
    const result = await refreshReleases(scoped)
    if (result.errors.length) message?.error(result.errors[0])
    return result.errors
  })
}

/**
 * 刷新全部：先清空列表，再重拉源树，最后逐个刷 Release。
 *
 * **必须清空**：刷新是「重新读源文档并重建列表」，旧卡片留在屏幕上会让人
 * 以为只是部分条目在更新，而且旧对象在被替换前仍然可点（可能已经下线或改名）。
 * 这与 MOD 安装页是同一条逻辑 —— 那边由 `refreshEverything()` 先
 * `clearRepositorySourceCache` 把列表清空。
 *
 * 失败时把旧列表还原：源文档拉不到就白屏，用户会以为地图全没了；留着旧数据
 * 至少能看、能装，下一次刷新再试。
 */
async function refreshAll(
  options: { countUpdated?: () => number; priorityIds?: ReadonlySet<string> } = {},
) {
  if (refreshingAll.value) return
  state.beginBatch('full')
  const previous = maps.value
  maps.value = []
  try {
    let sourceError: string | null = null
    try {
      maps.value = await api.refreshMapSources()
    } catch (reason) {
      maps.value = previous
      sourceError = backendMessageOf(reason)
      message?.error(sourceError)
    }
    /*
     * trigger 传 `single`：整批的结果由下面统一写一条 `full`，
     * 否则这里会先把「上次检查」覆盖成半批数据（源树阶段的失败还没并进来）。
     */
    const result = await refreshReleases(maps.value, {
      trigger: 'single',
      priorityIds: options.priorityIds,
      countUpdated: options.countUpdated,
    })
    result.errors.forEach((error) => message?.error(error))
    /*
     * 源树阶段与条目阶段的错误在这里合并写成一条结果：源文档失败时
     * 条目一条都没刷（`refreshed` 会是 0），摘要要能同时说清这两件事。
     */
    state.publishOutcome({
      trigger: 'full',
      refreshed: result.refreshed,
      updated: result.updated,
      errors: sourceError ? [sourceError, ...result.errors] : result.errors,
      finishedAt: Date.now(),
    })
  } finally {
    state.endBatch('full')
  }
}

export function useMapSources() {
  if (!message) message = useNotify()
  return {
    maps,
    loading,
    refreshing,
    queued,
    refreshingSources,
    installing,
    refreshingAll,
    checkingBound,
    anyRefreshing,
    outcome,
    loadMaps,
    refreshAll,
    refreshSource,
    refreshReleases,
    refreshBoundMaps,
    setFlag,
    syncDependencySnapshots,
  }
}
