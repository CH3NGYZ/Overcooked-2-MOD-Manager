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
 * MOD 安装页没有这个问题，因为它的刷新状态在 `useModRepositoryRefresh` 里同样
 * 是模块级单例。这里与之对齐：数据与状态都在模块作用域，页面只是渲染它们。
 *
 * ## 与 `useModRepositoryRefresh` 的差别
 *
 * MOD 侧的数据源是 Dashboard（`game`/`mods`/`config`），刷新引擎只持有
 * 「谁在刷」。地图侧的数据是独立的一份（`maps`），没有对应的 store，
 * 所以数据和引擎一起放在这里。
 */
import { computed, ref } from 'vue'
import { api } from '../services/tauri'
import { createSourceTreeState } from '../sourceTreeState'
import { createRefreshState, type RefreshOutcome, type RefreshTrigger } from '../refreshState'
import { prioritizeByIds, runReleaseBatch, runSourceReleaseRefresh, type ReleaseFetchSpec } from '../releaseRefresh'
import { backendMessageOf } from '../locale/backend'
import { useNotify, type Notify } from '../useNotify'
import { useLevelSetScan } from './useLevelSetScan'
import { mapForLevelSet, type LevelSetRefreshingHint } from '../levelSetStatus'
import { demoActive, isDemoId } from '../tutorial/demo'
import type { MapInfo, SourceNode } from '../types'

/** 并行上限（4 组），与仓库侧 `useModRepositoryRefresh` 的取值保持一致。 */
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
// Read-only snapshots may overlap source refresh or per-card writes. A late
// snapshot must never roll those newer results back.
let mapsRevision = 0
/** 首次读数据中（只影响整页加载态，不影响刷新）。 */
const loading = ref(true)
/** 每张地图的安装状态。安装进行中切走再回来也要保持。 */
const installing = ref<Record<string, boolean>>({})

/**
 * 刷新状态（与仓库侧 `useModRepositoryRefresh` 同一份实现 `createRefreshState`）。
 *
 * 条目级 `refreshing`、源级 `refreshingSources`、整批 `refreshingAll`、
 * 检查更新 `checkingBound` 与结果 `outcome` 全部来自它 —— 两套刷新的**规则**
 * 只有一处，不会一边按 4 并发、另一边按串行，也不会一边算成功、另一边算失败。
 */
const state = createRefreshState(REFRESH_CONCURRENCY)
const sourceTree = createSourceTreeState()
/** Persisted map snapshots can display their source notes before a tree refresh. */
const sourceNodes = computed<SourceNode[]>(() => {
  if (sourceTree.nodes.value.length) return sourceTree.nodes.value
  const known = new Map<string, SourceNode>()
  for (const map of maps.value) {
    if (!map.source_id || known.has(map.source_id)) continue
    known.set(map.source_id, { id: map.source_id, label: map.source_name || map.source_id,
      label_i18n: map.source_name_i18n, note_i18n: map.source_note_i18n, status: 'ready' })
  }
  return [...known.values()]
})
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
 * 检查更新会重读源树但不清空列表。两个页面读同一个标志，
 * 所以从地图管理页点「检查更新」时，下载页的刷新按钮也会跟着禁用。
 */
const checkingBound = state.checkingBound
/** 最近一次批次结果（地图管理页的「上次检查」摘要读它）。 */
const outcome = state.outcome

/** 任一刷新活动：两个页面的按钮 loading / 禁用统一看它。 */
const anyRefreshing = state.anyRefreshing
const refreshingHint = computed<LevelSetRefreshingHint>(() => ({
  entryStateUnavailable: !demoActive.value && (entryStateUnavailable.value || state.preparingSources.value),
  maps: new Set(Object.keys(refreshing.value)),
  queued: new Set(Object.keys(queued.value)),
  sources: new Set(Object.keys(refreshingSources.value)),
}))

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
  if (demoActive.value) return
  if ((refreshingAll.value || state.preparingSources.value) && !options.force) return
  const revision = ++mapsRevision
  loading.value = maps.value.length === 0
  try {
    const result = await api.maps()
    if (revision === mapsRevision) maps.value = result
  } catch (reason) {
    message?.error(backendMessageOf(reason))
  } finally {
    loading.value = false
  }
}

/** Startup uses the same source → Release/update pipeline as full refresh,
 * with normal TTL caching. It runs once independently of page lifetimes. */
let startupState: 'idle' | 'running' | 'done' = 'idle'
export function requestMapSourcesStartupRefresh() {
  if (demoActive.value) return
  if (startupState !== 'idle') return
  startupState = 'running'
  void refreshAll()
    .finally(() => {
      loading.value = false
      startupState = 'done'
    })
}

async function syncDependencySnapshots() {
  if (demoActive.value) return
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
function mapReleaseSpec(passId: string, force = false): ReleaseFetchSpec<MapInfo> {
  const skipped = new Set<string>()
  return {
    // Only the floating full-refresh button bypasses TTL; same-pass reads still coalesce.
    // Queued real jobs may reach the worker after the tutorial has started.
    fetch: (item) => {
      if (demoActive.value || isDemoId(item.id)) {
        skipped.add(item.id)
        return Promise.resolve(item.cached_releases ?? [])
      }
      return api.refreshMapReleases(item.id, force, passId)
    },
    /*
     * 成功写回只替换这一张图，不等整批结束重载列表：`latest_tag` 取自缓存的
     * 第一条，与后端 `map_to_info` 完全同一条规则（所以不必在前端另立判定）。
     */
    apply: (item, releases) => {
      if (skipped.has(item.id)) return
      patchMap(item.id, {
        cached_releases: releases,
        latest_release: releases[0] ?? null,
        latest_tag: releases[0]?.tag_name ?? null,
        cached_release_count: releases.length,
        cached_at: Date.now(),
        last_error: null,
      })
    },
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
  mapsRevision += 1
  const next = maps.value.slice()
  next[index] = { ...next[index], ...patch }
  maps.value = next
}

async function refreshReleases(
  list: MapInfo[],
  options: {
    force?: boolean
    trigger?: RefreshTrigger
    /** 已装地图的 id：排到队首，先出结论。 */
    priorityIds?: ReadonlySet<string>
    countUpdated?: () => number
    /**
     * 同一个用户动作的批次号：调用方已经生成了就透传（源树与 Release 必须
     * 同一批），单点刷新这类独立动作不传，由这里新生成一个。
     */
    passId?: string
    /** This action already expanded release-tags from its repository snapshot. */
    sourcePrepared?: boolean
  } = {},
): Promise<RefreshOutcome> {
  if (demoActive.value) return { trigger: options.trigger ?? 'partial', refreshed: 0, updated: 0, errors: [], finishedAt: Date.now() }
  const priorityIds = options.priorityIds ?? new Set(
    useLevelSetScan().levelSets.value.flatMap((set) => {
      const map = mapForLevelSet(set, maps.value)
      return map ? [map.id] : []
    }),
  )
  const passId = options.passId ?? newPassId()
  const available = list.filter((map) => !isDemoId(map.id) && !installing.value[map.id])
  // Source expansion has already published these cards' declarations, versions
  // and errors. Do not fan them out into a second IPC or reactive write queue.
  const prepared = options.sourcePrepared ? available.filter(map => map.release_derived) : []
  const result = await runReleaseBatch<MapInfo>({
    state,
    // 正在安装的不能同时刷：安装要替换目录，刷新会读它的元数据
    items: available.filter(map => !options.sourcePrepared || !map.release_derived),
    ...mapReleaseSpec(passId, options.force),
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
  return {
    ...result,
    refreshed: result.refreshed + prepared.length,
    errors: [...prepared.flatMap(map => map.last_error ? [backendMessageOf(map.last_error)] : []), ...result.errors],
  }
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

/** Load all map declarations first. Bundle entries project their versions from
 * the shared repository snapshot; ordinary single-map sources use the same API
 * refresh queue. Failed nodes stay empty and never roll successful siblings back. */
async function prepareMapSources(passId: string, force = false, sourceId?: string): Promise<string[]> {
  if (demoActive.value) return []
  mapsRevision += 1
  try {
    const result = await api.refreshMapSources(passId, force, progress => {
      mapsRevision += 1
      sourceTree.replace(progress.nodes)
      maps.value = progress.maps
    }, sourceId)
    mapsRevision += 1
    maps.value = result
    return sourceTree.nodes.value.flatMap(node => node.error ? [backendMessageOf(node.error)] : [])
  } catch (reason) {
    return [backendMessageOf(reason)]
  }
}

async function refreshBoundMaps(
  targets: MapInfo[],
  options: { countUpdated?: () => number } = {},
): Promise<MapBoundRefreshSummary> {
  if (demoActive.value || anyRefreshing.value) return { refreshed: 0, updated: 0, errors: [] }
  const passId = newPassId()
  const ids = new Set(targets.map((map) => map.id))
  const result = await runSourceReleaseRefresh({
    state,
    trigger: 'bound',
    prepare: () => prepareMapSources(passId),
    refresh: () => refreshReleases(maps.value.filter((map) => ids.has(map.id)), {
      trigger: 'single', passId, sourcePrepared: true, ...options,
    }),
  })
  return { refreshed: result.refreshed, updated: result.updated, errors: result.errors }
}

async function refreshSource(node: { id: string }) {
  if (demoActive.value || isDemoId(node.id) || anyRefreshing.value) return
  const passId = newPassId()
  const removed = sourceTree.begin(node.id)
  mapsRevision += 1
  maps.value = maps.value.filter(map => !map.source_id || !removed.has(map.source_id))
  const result = await runSourceReleaseRefresh({
    state,
    trigger: 'partial',
    prepare: () => state.runSource(node.id, () => prepareMapSources(passId, false, node.id)),
    refresh: () => refreshReleases(maps.value.filter((map) =>
      sourceTree.descendants(node.id).has(map.source_id || '__none__')), { trigger: 'single', passId, sourcePrepared: true }),
  })
  if (result.errors.length) message?.error(result.errors[0])
}

async function refreshAll(
  options: { countUpdated?: () => number; priorityIds?: ReadonlySet<string>; force?: boolean } = {},
) {
  if (demoActive.value || anyRefreshing.value) return
  const passId = newPassId()
  state.setEntryStateUnavailable(true)
  maps.value = []
  sourceTree.replace([])
  const result = await runSourceReleaseRefresh({
    state,
    trigger: 'full',
    prepare: async () => {
      const errors = await prepareMapSources(passId, options.force)
      return errors
    },
    refresh: () => refreshReleases(maps.value, { ...options, trigger: 'single', passId, sourcePrepared: true }),
  })
  result.errors.forEach((error) => message?.error(error))
}

export function useMapSources() {
  if (!message) message = useNotify()
  return {
    maps,
    loading: computed(() => !demoActive.value && loading.value),
    sourceNodes,
    refreshing,
    queued,
    preparingSources: computed(() => !demoActive.value && state.preparingSources.value),
    refreshingSources,
    installing,
    refreshingAll: computed(() => !demoActive.value && refreshingAll.value),
    entryStateUnavailable,
    checkingBound: computed(() => !demoActive.value && checkingBound.value),
    anyRefreshing: computed(() => !demoActive.value && anyRefreshing.value), refreshingHint,
    outcome: computed(() => demoActive.value ? null : outcome.value),
    loadMaps,
    refreshAll,
    refreshSource,
    refreshReleases,
    refreshBoundMaps,
    setFlag,
    syncDependencySnapshots,
  }
}
