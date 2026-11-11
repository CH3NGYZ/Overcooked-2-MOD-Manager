import { computed, ref, watch } from 'vue'
import { api } from '../services/tauri'
import { createSourceTreeState } from '../sourceTreeState'
import type { RuntimeRefreshState } from '../modRuntimeStatus'
import { modUpdateAvailable } from '../statusTags'
import { createRefreshState, type RefreshOutcome, type RefreshTrigger } from '../refreshState'
import { refreshReleaseItem, runReleaseBatch, runSourceReleaseRefresh, type ReleaseFetchSpec } from '../releaseRefresh'
import { pickI18n } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { useDashboard } from './useDashboardStore'
import { prioritizeInstalledModRepositories } from '../modRepositoryPriority'
import { demoActive, isDemoId } from '../tutorial/demo'
import {
  modRepositoryIsAvailable, sourceMatchesRegion,
} from '../repositoryAvailability'
import type { ModRepository, RepositorySource } from '../types'

/**
 * 仓库刷新引擎（模块级单例）。
 *
 * 从 MOD 安装页抽出来，是为了让「安装器启动刷新」与页面切换解耦：
 * 页面每次进入只读数据，刷新只由手动按钮或启动流程触发一次。
 *
 * 刷新顺序固定为：父源文档（raw TOML）→ 它声明的子源文档 → 全部仓库 Release（并发 4）。
 * 父源文档只声明子源、仓库在各子源文档里，所以必须递归加载子源，
 * 否则父源清空旧内容后不会补回任何仓库。
 *
 * ## 状态从哪来
 *
 * 标志、并发、结果全部来自 `createRefreshState` —— 与地图侧
 * （`useMapSources`）用的是同一份实现。这个文件只保留**仓库域特有**的东西：
 * 源文档递归链、内置快照回退、社区仓库发现、已安装优先排序。
 */

/** 社区仓库源由自动搜索管理，不参与文档刷新。 */
const COMMUNITY_SOURCE_IDS = new Set(['github-community', 'gitee-community', 'gitcode-community'])
/**
 * 并行上限（4 组）。
 *
 * 源文档阶段与 Release 阶段共用同一个数字：两段都会真的发网络请求，
 * 各留一个常量迟早一边调了另一边没调，实际并发变成乘积。
 */
const REFRESH_CONCURRENCY = 4

/**
 * 刷新状态（模块作用域 = 单例）。
 *
 * 为什么必须是模块级而不是 `provide/inject` 或页面内 `ref`：
 * 启动刷新在 `App.vue`、手动刷新在 MOD 安装页、检查更新在 MOD 管理页 ——
 * 同一批刷新要在三个页面之间保持同一份状态，且刷新中切页往返不能丢。
 */
const state = createRefreshState(REFRESH_CONCURRENCY)
const sourceTree = createSourceTreeState()
/** 仓库 Release 正在刷新（按仓库 id）。 */
const refreshing = state.refreshing
/** 排队中（已在这一批里、还没轮到）：界面上与「刷新中」同等对待。 */
const queued = state.queued
/** 源文档正在刷新（按源 id，含父源与子源）。 */
const sourceRefreshing = state.refreshingSources
/** 整批刷新进行中（只有「刷新全部」这一条入口会置真）。 */
const refreshingAll = state.refreshingAll
/**
 * 条目集合正在清空/重建（「刷新全部」的源文档与社区发现阶段）。
 *
 * 卡片标签只认这个、不认 `refreshingAll`：整批刷新会 `clearRepositorySourceCache`
 * 把仓库列表清空，那段时间按仓库 id 的状态一个都匹配不上，只能靠它兜住；
 * 而列表一填回来（`reloadConfig` 之后、Release 阶段开始前）就必须关掉，
 * 让条目级的 `refreshing` / `queued` 逐个接管 —— 否则整批标志会把「刷一个
 * 回来一个」压成「整批一起回来」。
 */
const entryStateUnavailable = state.entryStateUnavailable
/**
 * 「检查更新」批次进行中（只刷新与本地已安装条目绑定的仓库）。
 *
 * 与 `refreshingAll` 分开是因为两者的**副作用范围**不同：
 * `refreshingAll` 会先清空源缓存、让安装页整页进入加载态；
 * 检查更新会重读源文档但不清空列表，再刷新已绑定仓库的 Release。
 * 混用一个标志会让 MOD 管理页点一下「检查」就把 MOD 安装页刷成白屏。
 *
 * 放在模块作用域（而不是页面里的 `ref`）的原因与其它状态相同：
 * 刷新中切走再切回来，按钮要还是 loading，否则用户会以为已经结束。
 */
const checkingBound = state.checkingBound
/** 最近一次批次结果（管理页的「上次检查」摘要读它）。 */
const outcome = state.outcome

/** 任一刷新活动：按钮 loading/禁用统一看它。 */
const anyRefreshing = state.anyRefreshing

/**
 * 「有仓库正在加载」——只用于决定刷新按钮的图标要不要转。
 *
 * 语义与 `anyRefreshing` 不同，两者不能合并：
 *
 *   · `anyRefreshing` 是「整条刷新流水线忙不忙」，含**源文档**阶段
 *     （拉索引、递归子源）。它用于禁用按钮，防止重复触发。
 *   · `repositoriesLoading` 只关心**仓库本身**是否在加载。源文档阶段一个
 *     仓库都还没开始刷，图标不该转 —— 用户看到的应当是「点了之后马上转、
 *     一直转到最后一个仓库完成或失败」。
 *
 * 所以这里看的是「当前有几个仓库在刷」（`inFlight`），而不是
 * `refreshingAll`（它在源文档阶段就已经是 true，会持续几十秒）。
 *
 * 它是模块级单例派生，所以「启动时正在刷新」「在 MOD 管理页点了刷新」
 * 都会让 MOD 安装页的同一个按钮转起来 —— 与从哪个页面发起无关。
 */
const repositoriesLoading = computed(() => state.inFlight.value > 0)

const runtimeState = computed<RuntimeRefreshState>(() => ({
  refreshing: refreshing.value,
  queued: queued.value,
  sourceRefreshing: sourceRefreshing.value,
  entryStateUnavailable: !demoActive.value && (entryStateUnavailable.value || state.preparingSources.value),
}))

// Card comparisons are incremental (runtimeState). This is only the final
// aggregate summary over the same local snapshot; it never fetches Releases.
const dashboardStore = useDashboard()
watch([anyRefreshing, dashboardStore.modsStatus, dashboardStore.mods, dashboardStore.config],
  ([busy, status]) => {
    dashboardStore.invalidateModUpdates()
    if (demoActive.value) return
    if (!busy && status === 'ready' && dashboardStore.dashboard.value) void dashboardStore.checkModUpdates()
  }, { flush: 'post' })

/** 最近一次整批刷新的失败信息（供页面决定是否提示）。 */
const lastErrors = ref<string[]>([])

/**
 * 启动刷新的门控状态：
 *   idle     → 还没安排（页面自行决定）
 *   deferred → 被教程门控暂缓（V2 前端教程已裁剪，目前没有调用方）
 *   running  → 已开始
 *   done     → 已完成，不再自动触发
 */
type StartupState = 'idle' | 'deferred' | 'running' | 'done'
let startupState: StartupState = 'idle'

function store() {
  const { loadBase, reloadConfig, applyConfigSnapshot, config, mods } = useDashboard()
  const sources = computed<RepositorySource[]>(() => config.value?.repository_sources ?? [])
  const repositories = computed<ModRepository[]>(() => config.value?.repositories ?? [])
  /** 当前地区决定哪些源/仓库真实存在；空值按「非中国大陆」处理，与后端一致。 */
  const region = computed(() => config.value?.region)
  /** 当前平台决定只看专属某平台的仓库（与 region 同语义层）。 */
  const platform = computed(() => config.value?.platform)
  return { loadBase, reloadConfig, applyConfigSnapshot, config, sources, repositories, mods, region, platform }
}

/**
 * 仓库域「刷一个条目」的全部回调。
 *
 * 单条刷新（`refreshOne`）与批量刷新（`runRepositoryBatch`）都用**这一份定义**：
 * 请求、成功写回、失败写回、错误文案只有一处，不会出现「单独点刷新会清
 * `last_error`、检查更新却不清」这种两套行为。
 *
 * Release 结果逐条写回，版本判定直接使用本地扫描快照，不重扫 DLL。
 * 只有右下角整批刷新显式传 force；其它入口复用五分钟缓存。
 */
function repositoryReleaseSpec(options: { force?: boolean } = {}): ReleaseFetchSpec<ModRepository> {
  const force = options.force ?? false
  const skipped = new Set<string>()
  return {
    fetch: (item) => {
      if (demoActive.value || isDemoId(item.id)) {
        skipped.add(item.id)
        return Promise.resolve(item.cached_releases ?? [])
      }
      return api.refreshModRepository(item.id, force)
    },
    apply: (item, releases) => {
      if (skipped.has(item.id)) return
      patchRepository(item.id, {
        cached_releases: releases,
        last_error: undefined,
      })
    },
    /*
     * 失败写回与后端**同一条规则**（`refresh_mod_repository` 的 Err 分支）：
     * 缓存清空 + 置 `last_error`。不这么写的话，前端会保留旧 Release 并显示
     * 「最新版」，直到整批结束的后端重扫才纠正。
     */
    fail: (item, message) => patchRepository(item.id, {
      cached_releases: [],
      last_error: message,
    }),
    // 原因来自后端，可能是 tag —— 拼进展示串之前必须翻译
    errorText: (item, reason) => `${item.name}: ${backendMessageOf(reason)}`,
  }
}

/** 单仓刷新（「刷一个回来一个」的最小单位）。 */
async function refreshOne(repo: ModRepository, force = false): Promise<string | null> {
  if (demoActive.value || isDemoId(repo.id)) return null
  return refreshReleaseItem(state, repo, repositoryReleaseSpec({ force }))
}

/**
 * 只替换 Dashboard 快照里的**一个仓库**，其余仓库、MOD 列表和游戏状态沿用原对象。
 *
 * dashboard 是 shallowRef：必须整体替换引用才会触发视图更新；只重建这一条
 * 仓库，就不会触发整页数据替换带来的重绘卡顿。成功与失败都走这里 ——
 * 失败路径早先是等整批 `load()`，于是同一批里成功的卡片一个个变、
 * 失败的还挂着旧结论。
 */
function patchRepository(repositoryId: string, patch: Partial<ModRepository>) {
  const { dashboard, applyConfigSnapshot } = useDashboard()
  const current = dashboard.value
  if (!current) return
  const config = current.config
  const index = config.repositories.findIndex((repo) => repo.id === repositoryId)
  if (index < 0) return
  const repositories = config.repositories.slice()
  repositories[index] = { ...repositories[index], ...patch }
  applyConfigSnapshot({ ...config, repositories })
}

/**
 * 把「有本地已安装 MOD 的仓库」排到队列最前，其余保持原有顺序。
 * 实现见 `src/modRepositoryPriority.ts`（纯函数，便于直接测试）。
 *
 * **排序下沉到这里**，而不是留给调用方：刷新入口有四个（刷新全部、整源、
 * 分组、检查更新），只有一处排序迟早漏掉三个 —— 用户在安装页点整源刷新时，
 * 自己装的仓库会排在几十个未安装仓库后面慢慢等，看起来就像「刷新没生效」。
 */

/**
 * 一次批量刷新（本域唯一的实现，四个入口都走它）。
 *
 * 单条结果立即写回；已安装绑定优先，收尾只重读配置，不重新扫描本地文件。
 */
async function runRepositoryBatch(
  repos: ModRepository[],
  options: { force?: boolean; trigger?: RefreshTrigger } = {},
): Promise<RefreshOutcome> {
  if (demoActive.value) return { trigger: options.trigger ?? 'partial', refreshed: 0, updated: 0, errors: [], finishedAt: Date.now() }
  const { mods } = store()
  /*
   * 仓库文件（RAW）模式没有 Release 可刷，直接跳过，不当成失败上报。
   * 已安装优先让用户自己装的仓库先出结论。
   */
  const refreshable = repos.filter((repo) => !isDemoId(repo.id) && repo.download_mode !== 'repository_file')
  return runReleaseBatch<ModRepository>({
    state,
    items: refreshable,
    ...repositoryReleaseSpec({ force: options.force }),
    trigger: options.trigger ?? 'partial',
    prioritize: (items) => prioritizeInstalledModRepositories([...items], mods.value),
    finalize: () => store().reloadConfig(),
    // Summarize the same local snapshot and Release results; no second API pass.
    countUpdated: () => store().mods.value.filter((mod) => modUpdateAvailable(mod, store().repositories.value)).length,
  })
}

/**
 * 限并发的批量刷新（旧签名：调用方只取失败文案）。
 *
 * `force` 的默认值必须是 false：启动刷新与切页触发的刷新要尽量吃 5 分钟
 * Release 缓存；仅右下角整批刷新显式绕过它。
 */
async function refreshMany(repos: ModRepository[], options: { force?: boolean } = {}) {
  const result = await runRepositoryBatch(repos, { force: options.force, trigger: 'partial' })
  return result.errors
}

/** 一次「检查更新」的结果：刷新了多少个仓库、其中多少有新版、哪些失败。 */
export interface BoundRefreshSummary {
  /** 实际进入刷新队列的仓库数（已跳过正在刷新中的与 RAW 模式的）。 */
  refreshed: number
  /** 刷新后判定为「有新版」的 MOD 数。 */
  updated: number
  /** 失败文案（`<仓库名>: <原因>`），调用方逐条提示。 */
  errors: string[]
}

/** Prepare all source TOMLs first, then refresh only the bound repositories.
 * API refresh supplies update evidence directly; ordinary checks honor cache TTL. */
async function refreshBoundRepositories(repos: ModRepository[]): Promise<BoundRefreshSummary> {
  if (demoActive.value || anyRefreshing.value) return { refreshed: 0, updated: 0, errors: [] }
  const ids = new Set(repos.map((repo) => repo.id))
  const result = await runSourceReleaseRefresh({
    state,
    trigger: 'bound',
    prepare: () => prepareRepositorySources(false),
    refresh: () => runRepositoryBatch(store().repositories.value.filter((repo) => ids.has(repo.id)), { trigger: 'single' }),
  })
  return { refreshed: result.refreshed, updated: result.updated, errors: result.errors }
}

/** 源节点和错误提示共用当前语言的源昵称。 */
function sourceDisplayName(source: RepositorySource): string {
  return pickI18n(source.nickname_i18n, source.nickname)
}

/**
 * 把「来源名 + 失败原因」拼成一条可展示的错误。
 *
 * 收口点在这里是因为单仓刷新和源链刷新都是同一个
 * 形态，而**原因部分是后端 tag**（`source.tomlInvalid` 之类）。谁拼这条
 * 字符串谁就必须翻译，否则界面上会出现「某仓库: source.tomlInvalid」。
 */
function sourceError(source: RepositorySource, reason: string): string {
  return `${sourceDisplayName(source)}: ${translateBackendMessage(reason)}`
}

/**
 * 刷新一个源文档，并递归刷新它本次声明出来的子源。
 * 父源文档应用后必须重新读取配置才能看到新子源，所以每层都要重读一次。
 *
 * 这里用 `reloadConfig()` 而不是 `load()`：前者只取最新配置（仓库与源列表），
 * 不重新探测游戏目录、不重新扫描 DLL。一次刷新要经过父源 + 多个子源，
 * 用 load() 会把全量扫描重复十几次，这是刷新期间卡顿的主要来源。
 */
async function refreshSourceChain(sourceId: string, visited = new Set<string>()): Promise<string[]> {
  if (demoActive.value || isDemoId(sourceId)) return []
  if (visited.has(sourceId)) return []
  visited.add(sourceId)
  const { reloadConfig, sources, region } = store()
  const source = sources.value.find((entry) => entry.id === sourceId)
  if (!source) return []

  sourceTree.declare(sources.value.filter(item => !COMMUNITY_SOURCE_IDS.has(item.id)).map(item => ({
    id: item.id, parent_id: item.parent_id, label: item.nickname, label_i18n: item.nickname_i18n,
    url: item.url, status: 'pending' as const,
  })))
  const removed = sourceTree.begin(sourceId)
  const snapshot = store().config.value
  if (snapshot) store().applyConfigSnapshot({ ...snapshot,
    repositories: snapshot.repositories.filter(repo => !repo.source_id || !removed.has(repo.source_id)),
    repository_sources: snapshot.repository_sources.filter(item => item.id === sourceId || !removed.has(item.id)),
  })
  /* 源级标志也走共享状态机：地图侧用的是同一套写入规则 */
  const importErrors = await state.runSource(sourceId, async () => {
    try {
      const result = await api.importRepositorySource(source.id, source.url)
      if (result.access_error) return [sourceError(source, result.access_error)]
      // 配置回读也是这份源的应用阶段：失败必须留在源分组，不能以空列表报成功。
      await reloadConfig({ throwOnError: true })
      return []
    } catch (reason) {
      return [sourceError(source, reason instanceof Error ? reason.message : String(reason))]
    }
  })
  // 父源没读出来时它的子源无从谈起：直接返回，别拿旧 `sources` 递归
  if (importErrors.length) { sourceTree.settle(sourceId, importErrors[0]); return importErrors }

  sourceTree.declare(sources.value.filter(item => !COMMUNITY_SOURCE_IDS.has(item.id)).map(item => ({
    id: item.id, parent_id: item.parent_id, label: item.nickname, label_i18n: item.nickname_i18n,
    url: item.url, status: 'pending' as const,
  })))
  sourceTree.settle(sourceId)
  // 子源同样按地区过滤：父源是通用的，但它可以声明地区专属子源
  const children = sources.value.filter(
    (entry) => entry.parent_id === sourceId && sourceMatchesRegion(entry, region.value),
  )
  if (!children.length) return []
  const childErrors: string[] = []
  for (const child of children) childErrors.push(...await refreshSourceChain(child.id, visited))
  return childErrors
}

async function refreshSourceDocuments(): Promise<string[]> {
  if (demoActive.value) return []
  const { sources, region } = store()
  const refreshable = sources.value.filter(
    (source) => !COMMUNITY_SOURCE_IDS.has(source.id) && sourceMatchesRegion(source, region.value),
  )
  const known = new Set(refreshable.map((source) => source.id))
  // 父源已被移除的孤儿当作根源，避免整条链都不刷新
  const roots = refreshable.filter((source) => !source.parent_id || !known.has(source.parent_id))
  if (!roots.length) return []
  const visited = new Set<string>()
  // Apply each declaration before descending. Concurrent config snapshots can
  // discard a just-created child and leave its node pending forever.
  const errors: string[] = []
  for (const source of roots) errors.push(...await refreshSourceChain(source.id, visited))
  return errors
}

/**
 * 完整刷新：源文档（父→子）→ 全部启用仓库的 Release（并发 4）。
 * 返回失败清单；调用方决定是否提示。
 */
async function prepareRepositorySources(clearEntries: boolean): Promise<string[]> {
    if (demoActive.value) return []
    const { loadBase, reloadConfig, applyConfigSnapshot, repositories, sources, region } = store()
    // 启动时 Dashboard 可能还没读过：先读一次才有源和仓库可刷
    if (!sources.value.length && !repositories.value.length) await loadBase()
    if (demoActive.value) return []
    if (clearEntries) {
      sourceTree.replace([])
      const refreshable = sources.value.filter(
        (source) => !COMMUNITY_SOURCE_IDS.has(source.id) && sourceMatchesRegion(source, region.value),
      )
      if (refreshable.length) {
        try {
          const cleared = await api.clearRepositorySourceCache(refreshable.map((source) => source.id))
          applyConfigSnapshot(cleared)
        } catch (reason) {
          // 清空失败不阻塞刷新：后面每个源文档应用时也会各自清一次
          // 这条会被 message.error 直接显示，原因可能是后端 tag
          lastErrors.value.push(backendMessageOf(reason))
        }
      }
    }
    const sourceErrors = await refreshSourceDocuments()
    // 源文档阶段用的是 reloadConfig（只更新配置）；
    // 这里再读一次配置拿到最终仓库列表，Release 阶段结束时只重读配置，不扫描 DLL。
    await reloadConfig()
    return sourceErrors
}

async function refreshSource(sourceId: string): Promise<string[]> {
  if (demoActive.value || isDemoId(sourceId) || anyRefreshing.value) return []
  const result = await runSourceReleaseRefresh({
    state, trigger: 'partial',
    prepare: () => refreshSourceChain(sourceId),
    refresh: () => {
      const { repositories, sources, region, platform } = store()
      const ids = sourceTree.descendants(sourceId)
      return runRepositoryBatch(repositories.value.filter(repo => !!repo.source_id && ids.has(repo.source_id)
        && modRepositoryIsAvailable(repo, sources.value, region.value, platform.value)), { trigger: 'single' })
    },
  })
  return result.errors
}

async function refreshEverything(options: { force?: boolean } = {}): Promise<string[]> {
  if (demoActive.value || anyRefreshing.value) return []
  lastErrors.value = []
  state.setEntryStateUnavailable(true)
  const result = await runSourceReleaseRefresh({
    state,
    trigger: 'full',
    prepare: () => prepareRepositorySources(true),
    refresh: async () => {
      const { repositories, sources, region, platform } = store()
      const available = repositories.value.filter((repo) =>
        modRepositoryIsAvailable(repo, sources.value, region.value, platform.value))
      const result = await runRepositoryBatch(available, { trigger: 'single', force: options.force })
      if (!demoActive.value) await api.syncDependencySnapshots().catch(() => undefined)
      return result
    },
  })
  lastErrors.value = [...lastErrors.value, ...result.errors]
  return lastErrors.value
}

/**
 * 安排启动刷新（安装器启动时由 `App.vue` 调用一次）。
 *
 * V2 的前端教程系统已整体裁剪，所以启动后立即刷新，没有门控。
 * `deferStartupRefresh()` / `releaseStartupRefresh()` 是当时留下的门控脚手架，
 * 目前没有任何调用方；如果确定不再恢复教程，可以连同 `deferred` 状态一起删。
 */
function requestStartupRefresh() {
  if (demoActive.value) return
  if (startupState !== 'idle') return
  startupState = 'running'
  void refreshEverything().finally(() => { startupState = 'done' })
}

/** 教程开始时暂缓启动刷新（当前没有调用方，见 `requestStartupRefresh`）。 */
function deferStartupRefresh() {
  if (startupState !== 'idle') return
  startupState = 'deferred'
}

/** 教程结束/跳过时释放暂缓并立刻刷新（当前没有调用方，见 `requestStartupRefresh`）。 */
function releaseStartupRefresh() {
  if (startupState === 'deferred' || startupState === 'idle') requestStartupRefresh()
}

/** 是否还有待执行的启动刷新（页面可据此显示提示）。 */
const startupRefreshPending = computed(() => startupState === 'deferred' || startupState === 'idle')

export function useModRepositoryRefresh() {
  return {
    sourceNodes: sourceTree.nodes,
    preparingSources: computed(() => !demoActive.value && state.preparingSources.value),
    refreshing, queued, sourceRefreshing,
    refreshingAll: computed(() => !demoActive.value && refreshingAll.value), entryStateUnavailable, runtimeState,
    checkingBound: computed(() => !demoActive.value && checkingBound.value),
    anyRefreshing: computed(() => !demoActive.value && anyRefreshing.value),
    repositoriesLoading: computed(() => !demoActive.value && repositoriesLoading.value),
    outcome: computed(() => demoActive.value ? null : outcome.value), lastErrors, startupRefreshPending,
    refreshOne, refreshMany, refreshBoundRepositories,
    refreshSource, refreshSourceChain, refreshSourceDocuments, refreshEverything,
    requestStartupRefresh, deferStartupRefresh, releaseStartupRefresh,
  }
}
