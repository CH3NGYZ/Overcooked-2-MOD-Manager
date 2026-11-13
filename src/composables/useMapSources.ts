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
 * MOD 与地图两域都在模块级保留刷新状态，切页后继续订阅同一批任务。
 *
 * ## 与 `useModRepositoryRefresh` 的差别
 *
 * MOD 使用 Dashboard 的可信配置及引擎在线展示投影；地图列表与进度快照独立
 * 存放在这里。两域共用刷新状态机、源树状态与局部替换规则。
 */
import { computed, ref } from 'vue'
import { api } from '../services/tauri'
import { useDashboard } from './useDashboardStore'
import { createSourceTreeState, replaceScopedItems, sourceActivity, sourceDescendantIds } from '../sourceTreeState'
import { createRefreshState, type RefreshOutcome, type RefreshTrigger } from '../refreshState'
import { prioritizeByIds, runReleaseBatch, runSourceReleaseRefresh, type ReleaseFetchSpec } from '../releaseRefresh'
import { backendMessageOf } from '../locale/backend'
import { useNotify, type Notify } from '../useNotify'
import { useLevelSetScan } from './useLevelSetScan'
import { mapsForLevelSet, type LevelSetRefreshingHint } from '../levelSetStatus'
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
const refreshingSources = computed(() => sourceActivity(sourceTree.nodes.value, state.refreshingSources.value))
/** 整源刷新进行中（浮动按钮持续到源准备与 Release 队列结束）。 */
const refreshingAll = state.refreshingAll
/**
 * 列表正在重建（主文档成功后的源树阶段）。
 *
 * 卡片标签只认这个、不认 `refreshingAll`；主文档读取期间保留可信列表，成功后
 * 按源进度重新填入，那段时间用它兜住绑定状态；而列表一填回来
 * （`refreshReleases` 之前）就必须关掉，让条目级状态逐个接管 —— 否则整源
 * 标志会把「刷一个回来一个」压成「整批一起回来」。
 */
const entryStateUnavailable = state.entryStateUnavailable
/**
 * 「检查更新」批次进行中（只刷新本地已装地图绑定的那几张）。
 *
 * 与 `refreshingAll` 分开：整源刷新在主文档成功后逐源重建展示，
 * 检查更新保留列表。两个页面读同一个标志，
 * 所以从地图管理页点「检查更新」时，下载页的刷新按钮也会跟着禁用。
 */
const checkingBound = state.checkingBound
/** 最近一次批次结果（地图管理页的「上次检查」摘要读它）。 */
const outcome = state.outcome

/** 任一刷新活动：两个页面的按钮 loading / 禁用统一看它。 */
const anyRefreshing = state.anyRefreshing
const refreshingHint = computed<LevelSetRefreshingHint>(() => ({
  entryStateUnavailable: !demoActive.value && (entryStateUnavailable.value || state.preparingAllSources.value),
  maps: new Set(Object.keys(refreshing.value)),
  queued: new Set(Object.keys(queued.value)),
  sources: new Set(Object.keys(refreshingSources.value)),
  sourceOrder: sourceNodes.value.map(node => node.id),
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
 *   · 页面挂载时读 —— 源准备或全量刷新中跳过，避免旧缓存覆盖本轮进度投影。
 *   · 明确要求读取时允许 force；revision 仍拒绝期间已被新条目写回淘汰的快照。
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
function mapReleaseSpec(passId: string, force = false): ReleaseFetchSpec<MapInfo, MapInfo> {
  const skipped = new Set<string>()
  return {
    // Only the floating full-refresh button bypasses TTL; same-pass reads still coalesce.
    // Queued real jobs may reach the worker after the tutorial has started.
    fetch: (item) => {
      if (demoActive.value || isDemoId(item.id)) {
        skipped.add(item.id)
        return Promise.resolve(item)
      }
      return api.refreshMapReleases(item.id, force, passId)
    },
    /*
     * 成功写回只替换这一张图，不等整批结束重载列表：`latest_tag` 取自缓存的
     * 第一条，与后端 `map_to_info` 完全同一条规则（所以不必在前端另立判定）。
     */
    apply: (item, snapshot) => {
      if (skipped.has(item.id)) return
      // The declaration, relationships and releases belong to one API snapshot.
      replaceMap(item.id, snapshot)
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
 * `maps` 是页面共同使用的列表，每张 Release 返回后立即写回。批次结束仅同步
 * 依赖快照，避免整表回读覆盖无关分组或刚完成的条目。
 */
function replaceMap(mapId: string, snapshot: MapInfo) {
  const index = maps.value.findIndex(map => map.id === mapId)
  if (index < 0) return
  mapsRevision += 1
  const next = maps.value.slice()
  next[index] = snapshot
  maps.value = next
}

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
    useLevelSetScan().levelSets.value.flatMap(set => mapsForLevelSet(set, maps.value).map(map => map.id)),
  )
  const passId = options.passId ?? newPassId()
  const available = list.filter((map) => !isDemoId(map.id) && !installing.value[map.id])
  // Source expansion has already published these cards' declarations, versions
  // and errors. Do not fan them out into a second IPC or reactive write queue.
  const prepared = options.sourcePrepared ? available.filter(map => map.release_derived) : []
  const result = await runReleaseBatch<MapInfo, MapInfo>({
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
async function prepareMapSources(passId: string, force = false, sourceId?: string, preparedIds?: Set<string>): Promise<string[]> {
  if (demoActive.value) return []
  mapsRevision += 1
  const branch = sourceId && sourceId !== 'official-map-index' ? sourceId : undefined
  let rootReady = !!branch
  let acceptProgress = true
  const scope = branch ? sourceTree.descendants(branch) : undefined
  const apply = (incoming: MapInfo[]) => {
    mapsRevision += 1
    if (scope) maps.value = replaceScopedItems(maps.value, incoming, map => !!map.source_id && scope.has(map.source_id))
    else maps.value = incoming
    if (preparedIds) {
      preparedIds.clear()
      for (const map of incoming) if (!scope || !!map.source_id && scope.has(map.source_id)) preparedIds.add(map.id)
    }
  }
  try {
    const result = await api.refreshMapSources(passId, force, progress => {
      if (!acceptProgress) return
      mapsRevision += 1
      if (branch && scope) {
        for (const id of sourceDescendantIds(progress.nodes, branch)) scope.add(id)
        sourceTree.replace(replaceScopedItems(sourceTree.nodes.value, progress.nodes, node => scope.has(node.id)))
      } else {
        const root = progress.nodes.find(node => !node.parent_id)
        rootReady ||= root?.status === 'ready'
        if (!rootReady) {
          // Main-file failure preserves the previous tree and cards. Its own
          // pending/error declaration is still visible and retryable.
          if (root) sourceTree.replace(replaceScopedItems(sourceTree.nodes.value, [root], node => node.id === root.id))
          return
        }
        sourceTree.replace(progress.nodes)
      }
      apply(progress.maps)
    }, sourceId)
    acceptProgress = false
    // A successful IPC result also covers a cache hit without intermediate events.
    apply(result)
    return sourceTree.nodes.value.flatMap(node => node.error && (!scope || scope.has(node.id)) ? [backendMessageOf(node.error)] : [])
  } catch (reason) {
    return [backendMessageOf(reason)]
  } finally {
    acceptProgress = false
    // The MOD root gate may have changed the active provider before map loading.
    await useDashboard().reloadConfig({
      project: (current, received) => ({ ...current, data_provider: received.data_provider }),
    })
  }
}

async function refreshBoundMaps(
  targets: MapInfo[],
  options: { countUpdated?: () => number } = {},
): Promise<MapBoundRefreshSummary> {
  if (demoActive.value || anyRefreshing.value) return { refreshed: 0, updated: 0, errors: [] }
  const passId = newPassId()
  const installed = useLevelSetScan().levelSets.value.filter(set => mapsForLevelSet(set, targets).length > 0)
  const result = await runSourceReleaseRefresh({
    state,
    trigger: 'bound',
    prepare: () => prepareMapSources(passId),
    refresh: () => refreshReleases(maps.value.filter(map => installed.some(set => mapsForLevelSet(set, [map]).length > 0)), {
      trigger: 'single', passId, sourcePrepared: true, ...options,
    }),
  })
  return { refreshed: result.refreshed, updated: result.updated, errors: result.errors }
}

async function refreshSource(node: { id: string }) {
  if (demoActive.value || isDemoId(node.id) || anyRefreshing.value) return
  const passId = newPassId()
  const preparedIds = new Set<string>()
  const removed = sourceTree.begin(node.id)
  mapsRevision += 1
  maps.value = maps.value.filter(map => !map.source_id || !removed.has(map.source_id))
  const result = await runSourceReleaseRefresh({
    state,
    trigger: 'partial',
    prepare: () => state.runSource(node.id, () => prepareMapSources(passId, false, node.id, preparedIds)),
    refresh: () => {
      const targets = maps.value.filter((map) => preparedIds.has(map.id))
      return refreshReleases(targets, { trigger: 'single', passId, sourcePrepared: true })
    },
  })
  if (result.errors.length) message?.error(result.errors[0])
}

async function refreshAll(
  options: { countUpdated?: () => number; priorityIds?: ReadonlySet<string>; force?: boolean } = {},
) {
  if (demoActive.value || anyRefreshing.value) return
  const passId = newPassId()
  const preparedIds = new Set<string>()
  state.setEntryStateUnavailable(true)
  const result = await runSourceReleaseRefresh({
    state,
    trigger: 'full',
    prepare: async () => {
      const errors = await prepareMapSources(passId, options.force, undefined, preparedIds)
      return errors
    },
    refresh: () => refreshReleases(maps.value.filter(map => preparedIds.has(map.id)), { ...options, trigger: 'single', passId, sourcePrepared: true }),
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
    preparingSources: computed(() => !demoActive.value && state.preparingAllSources.value),
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
