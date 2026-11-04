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

/**
 * 「同一次用户点击的刷新批次」编号（模块级递增）。
 *
 * tag 模式下，一个仓库的多个版本资产会被合成多条 Release；整源刷新一次会
 * 扇出几十张地图，其中常有多条来自同一个仓库。后端按这个编号把同批内同
 * 仓库的 Release 请求合并成一次，否则一次点击会按地图数打 API。
 *
 * 编号必须由**发起动作的地方**生成，并贯穿该动作的全部请求（源树 → Release）：
 * 在 `refreshReleases` 里各自现造就等于每批都是新批次，去重完全失效。
 */
let passSeq = 0
function newPassId(): string {
  passSeq += 1
  return `${Date.now().toString(36)}-${passSeq}`
}

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
 * 列表正在清空/重建（整源刷新的源树阶段）。
 *
 * 卡片标签只认这个、不认 `refreshingAll`：`refreshAll` 会把共享的 `maps`
 * 清空再重建，那段时间所有关卡集都查不到绑定，只能靠它兜住；而列表一填回来
 * （`refreshReleases` 之前）就必须关掉，让条目级状态逐个接管 —— 否则整源
 * 标志会把「刷一个回来一个」压成「整批一起回来」。
 */
const entryStateUnavailable = state.entryStateUnavailable
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

/**
 * 启动时静默重读一次地图源树。
 *
 * `release-tags` 源的地图不在内置快照里（要先联网读 Release note 才知道有
 * 哪几张图），不自动读的话新用户只看到单仓库地图，「把地图仓库写进源里就能
 * 用」这个承诺就落空了。
 *
 * 只读**源树**、不刷每张图的 Release：启动阶段替用户把几十个 Release 列表
 * 全拉一遍不值得，那部分仍由「刷新 / 刷新全部 / 检查更新」触发。
 *
 * 失败静默：启动时弹一条网络错误没有意义，用户手动刷新时会看到同一条。
 * 与 MOD 仓库的启动刷新同一条规则 —— 挂在 App 层，切页不重复触发。
 *
 * 导出成模块级函数而不是挂在 `useMapSources()` 上：调用方是 App.vue 的
 * setup，而 App 自己就是 `n-message-provider` 的提供者 —— 那里调
 * `useMapSources()` 会去 `useNotify()`，反过来要求 provider 已经挂好，
 * 组件树还没建就先崩了。这条链不需要消息实例。
 */
let startupState: 'idle' | 'running' | 'done' = 'idle'
export function requestMapSourcesStartupRefresh() {
  if (startupState !== 'idle') return
  startupState = 'running'
  const passId = newPassId()
  void api
    .refreshMapSources(passId)
    .then((list) => {
      maps.value = list
    })
    .catch(() => undefined)
    .finally(() => {
      loading.value = false
      startupState = 'done'
    })
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
function mapReleaseSpec(passId: string): ReleaseFetchSpec<MapInfo> {
  return {
    // 用户主动刷新 → 强制绕过 5 分钟 Release 缓存；同批同仓库的请求由后端按 passId 合并
    fetch: (item) => api.refreshMapReleases(item.id, true, passId),
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
    /**
     * 同一个用户动作的批次号：调用方已经生成了就透传（源树与 Release 必须
     * 同一批），单点刷新这类独立动作不传，由这里新生成一个。
     */
    passId?: string
  } = {},
): Promise<RefreshOutcome> {
  const priorityIds = options.priorityIds
  const passId = options.passId ?? newPassId()
  return runReleaseBatch<MapInfo>({
    state,
    // 正在安装的不能同时刷：安装要替换目录，刷新会读它的元数据
    items: list.filter((map) => !installing.value[map.id]),
    ...mapReleaseSpec(passId),
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
 * 检查更新：先重拉一次源树，再只刷新「本地已装关卡集绑定」的那几张地图，
 * **强制**绕过缓存。
 *
 * ## 为什么由引擎暴露
 *
 * 地图管理页原来自己 `for` 循环调 `api.refreshMapReleases`，状态挂在页面
 * 局部的 `checkingUpdates` 上 —— 于是同一件事在两页之间完全没有交汇：
 * 管理页在刷、下载页的卡片不转，切走再回来按钮还会『假装没在刷』。
 * 现在把入口收到这里，两个页面共用同一份 `refreshing` / `checkingBound`。
 *
 * ## 为什么先重拉源树
 *
 * tag 模式的地图条目把 `tag` / `latest_mapset_version` 写在源文档里，Release 列表
 * 是照这份声明合成的。拿页面上的旧条目直接去刷 Release，等于按上次的声明
 * 问后端；源文档改了 `asset_rules` 或换了 tag 时，刷回来的还是一份过期资产表。
 * 所以一次点击固定是「一次 TOML + 一次 Release 批次」，两者共用 `passId`，
 * 后端据此把同批内同一个仓库的 Release 请求合并成一次。
 *
 * 源树拉取失败时保留旧列表继续刷：比整次检查更新直接失败有用，至少还能
 * 确认本地装的是不是最新。重拉不清空列表（与整源刷新不同）：这里的语义
 * 是「顺手更新声明」，不是「重看一遍源」。
 */
async function refreshBoundMaps(
  targets: MapInfo[],
  options: { countUpdated?: () => number } = {},
): Promise<MapBoundRefreshSummary> {
  /*
   * 同一时刻只跑一批：整源刷新在重建列表时再重拉源树，两个响应会交替写
   * `maps`；自己已经在跑时再进来一批，等于把同一份源文档请求两次。
   * 页面按钮此时已经禁用，这里防的是同一帧内的连点。
   */
  if (refreshingAll.value || checkingBound.value) {
    return { refreshed: 0, updated: 0, errors: [] }
  }
  const passId = newPassId()
  /*
   * 批次标志要盖住整段动作（含源树阶段）：只靠 `runReleaseBatch` 打开标志，
   * 源树请求那几百毫秒里按钮还是可点的，连点就会把同一份 TOML 拉两遍。
   */
  state.beginBatch('bound')
  try {
    try {
      maps.value = await api.refreshMapSources(passId)
    } catch (reason) {
      // 不动 `maps.value`：旧列表还能看、能装，下一次刷新再试
      message?.error(backendMessageOf(reason))
    }
    /*
     * 按 id 取重拉后的条目：旧对象可能已被替换（版本、tag、资产规则都可能变），
     * 拿它刷 Release 会把结果写回一份已经过期的声明上。
     * 找不到（条目已下线）就退回原对象 —— 由后端报错，卡片显示刷新失败。
     *
     * 去重仍交给 `runBatch`（按 id 去重并跳过正在刷的），这里只筛掉安装中的：
     * 同一张图被两个页面同时点「检查更新」时，第二份请求没有意义。
     */
    const byId = new Map(maps.value.map((map) => [map.id, map]))
    const queue = targets
      .map((map) => byId.get(map.id) ?? map)
      .filter((map) => !installing.value[map.id])
    const result = await refreshReleases(queue, { trigger: 'bound', passId, ...options })
    return { refreshed: result.refreshed, updated: result.updated, errors: result.errors }
  } finally {
    state.endBatch('bound')
  }
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
  // 一次点击一个批次号：源树与随后这个源的 Release 必须被后端认成同一批
  const passId = newPassId()
  await state.runSource(node.id, async () => {
    try {
      maps.value = await api.refreshMapSources(passId)
    } catch (reason) {
      message?.error(backendMessageOf(reason))
    }
    const scoped = maps.value.filter((map) => (map.source_id || '__none__') === node.id)
    const result = await refreshReleases(scoped, { passId })
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
  // 一次点击只有一个批次号：源树阶段与 Release 阶段必须被后端认成同一批
  const passId = newPassId()
  state.beginBatch('full')
  /*
   * 列表清空到源树填回来之间，按地图 id 的状态一个都匹配不上：这段空窗期
   * 由 `entryStateUnavailable` 兜住卡片标签，Release 阶段开始前必须关掉。
   */
  state.setEntryStateUnavailable(true)
  const previous = maps.value
  maps.value = []
  try {
    let sourceError: string | null = null
    try {
      maps.value = await api.refreshMapSources(passId)
    } catch (reason) {
      maps.value = previous
      sourceError = backendMessageOf(reason)
      message?.error(sourceError)
    }
    /*
     * trigger 传 `single`：整批的结果由下面统一写一条 `full`，
     * 否则这里会先把「上次检查」覆盖成半批数据（源树阶段的失败还没并进来）。
     */
    /*
     * 列表已填回来（源树失败时还原成旧列表，条目级状态同样可用），
     * 从这里交给每条地图自己的状态 —— 保留整源标志会把所有卡片一起按住。
     */
    state.setEntryStateUnavailable(false)
    const result = await refreshReleases(maps.value, {
      trigger: 'single',
      passId,
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
    state.setEntryStateUnavailable(false)
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
    entryStateUnavailable,
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
