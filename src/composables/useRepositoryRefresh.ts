import { computed, ref } from 'vue'
import { api } from '../services/tauri'
import { providerLabel } from '../providers'
import { mapWithConcurrency } from '../concurrency'
import { createRefreshState, type RefreshOutcome, type RefreshTrigger } from '../refreshState'
import { refreshReleaseItem, runReleaseBatch, type ReleaseFetchSpec } from '../releaseRefresh'
import { pickI18n, locale, t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { useDashboard } from './useDashboardStore'
import { prioritizeInstalledRepositories } from '../repositoryPriority'
import {
  OFFICIAL_INDEX_ID, repositoryIsAvailable, shouldFallbackToBuiltin, sourceMatchesRegion,
} from '../repositoryAvailability'
import type { Provider, PublicConfig, Repository, RepositorySource } from '../types'

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
 * 检查更新不动源文档、不清列表，只是把几个仓库的 Release 重抓一遍。
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
  const { load, reloadConfig, applyConfigSnapshot, config, mods } = useDashboard()
  const sources = computed<RepositorySource[]>(() => config.value?.repository_sources ?? [])
  const repositories = computed<Repository[]>(() => config.value?.repositories ?? [])
  /** 当前地区决定哪些源/仓库真实存在；空值按「非中国大陆」处理，与后端一致。 */
  const region = computed(() => config.value?.region)
  /** 当前平台决定只看专属某平台的仓库（与 region 同语义层）。 */
  const platform = computed(() => config.value?.platform)
  return { load, reloadConfig, applyConfigSnapshot, config, sources, repositories, mods, region, platform }
}

/**
 * 仓库域「刷一个条目」的全部回调。
 *
 * 单条刷新（`refreshOne`）与批量刷新（`runRepositoryBatch`）都用**这一份定义**：
 * 请求、成功写回、失败写回、错误文案只有一处，不会出现「单独点刷新会清
 * `last_error`、检查更新却不清」这种两套行为。
 *
 * 这里刻意不调用 load()：全量重读会重新扫描游戏目录和每个 DLL，
 * 在滚动时刷新表现为明显卡顿。单个仓库的结果局部替换即可，
 * 整批结束后再统一 load() 一次补齐其它派生状态（更新数、MOD 标签等）。
 *
 * `force` 只由**用户手动点刷新**传入：后端有 5 分钟 Release 缓存，批量
 * 刷新走缓存才不会把整棵仓库树重打一遍；但用户点那颗刷新按钮时的语义
 * 就是「现在就要最新的」，必须绕过缓存。
 */
function repositoryReleaseSpec(options: { force?: boolean } = {}): ReleaseFetchSpec<Repository> {
  const force = options.force ?? false
  return {
    fetch: (item) => api.refreshRepository(item.id, force),
    apply: (item, releases) => patchRepository(item.id, {
      cached_releases: releases,
      last_error: undefined,
    }),
    /*
     * 失败写回与后端**同一条规则**（`refresh_repository` 的 Err 分支）：
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
async function refreshOne(repo: Repository, force = false): Promise<string | null> {
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
function patchRepository(repositoryId: string, patch: Partial<Repository>) {
  const { dashboard } = useDashboard()
  const current = dashboard.value
  if (!current) return
  const config = current.config
  const index = config.repositories.findIndex((repo) => repo.id === repositoryId)
  if (index < 0) return
  const repositories = config.repositories.slice()
  repositories[index] = { ...repositories[index], ...patch }
  dashboard.value = {
    ...current,
    config: { ...config, repositories },
  }
}

/**
 * 把「有本地已安装 MOD 的仓库」排到队列最前，其余保持原有顺序。
 * 实现见 `src/repositoryPriority.ts`（纯函数，便于直接测试）。
 *
 * **排序下沉到这里**，而不是留给调用方：刷新入口有四个（刷新全部、整源、
 * 分组、检查更新），只有一处排序迟早漏掉三个 —— 用户在安装页点整源刷新时，
 * 自己装的仓库会排在几十个未安装仓库后面慢慢等，看起来就像「刷新没生效」。
 */

/**
 * 一次批量刷新（本域唯一的实现，四个入口都走它）。
 *
 * 「刷一个显示一个」由局部更新实现：每个仓库完成时只替换它自己的
 * `cached_releases`（见 `refreshOne`），视图立刻显示该仓库的版本号，
 * 其余仓库继续转圈；整批结束后才 `load()` 一次，补齐更新数、MOD 标签等
 * 派生状态 —— 刷新过程中反复重读 Dashboard 会重新扫描游戏目录和每个 DLL，
 * 表现为页面滚动时明显卡一下。
 */
async function runRepositoryBatch(
  repos: Repository[],
  options: { force?: boolean; trigger?: RefreshTrigger } = {},
): Promise<RefreshOutcome> {
  const { mods } = store()
  /*
   * 仓库文件（RAW）模式没有 Release 可刷，直接跳过，不当成失败上报。
   * 已安装优先让用户自己装的仓库先出结论。
   */
  const refreshable = repos.filter((repo) => repo.download_mode !== 'repository_file')
  return runReleaseBatch<Repository>({
    state,
    items: refreshable,
    ...repositoryReleaseSpec({ force: options.force }),
    trigger: options.trigger ?? 'partial',
    prioritize: (items) => prioritizeInstalledRepositories([...items], mods.value),
    finalize: () => store().load(),
    /*
     * 「有更新」的数量只能在 `load()` 之后读：Dashboard 重读时后端会用刚拿到的
     * Release 缓存重算每个 MOD 的 `update_available`，之前读到的还是上一轮结论。
     */
    countUpdated: () => store().mods.value.filter((mod) => mod.update_available).length,
  })
}

/**
 * 限并发的批量刷新（旧签名：调用方只取失败文案）。
 *
 * `force` 的默认值必须是 false：启动刷新与切页触发的刷新要尽量吃 5 分钟
 * Release 缓存，只有用户手动点「刷新 / 检查更新」才值得绕过它。
 */
async function refreshMany(repos: Repository[], options: { force?: boolean } = {}) {
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

/**
 * 检查更新：只刷新「本地已安装条目绑定」的那几个仓库，**强制**绕过缓存。
 *
 * ## 为什么由引擎暴露，而不是页面自己调 `refreshMany`
 *
 * 三个页面（MOD 管理、MOD 安装、总览）读的是同一份模块级状态：
 * 谁发起刷新、从哪个页面发起，都不该改变别的页面看到的东西。
 * 页面自己拼调用顺序会出现「这个页在转、那个页不转」——
 * 本条入口保证「一批检查 = 一个共享的批次态（`checkingBound`）+ 每仓状态
 * + 一份可展示的结果（`outcome`）」。
 *
 * ## 为什么 force
 *
 * 用户点的是「检查更新」，语义就是「现在这版是不是最新的」。走 5 分钟缓存
 * 会让他刚发新版就点、点完还是旧结论，比多花一次请求糟糕得多。
 * 与地图管理页的「检查更新」保持同一取舍。
 */
async function refreshBoundRepositories(repos: Repository[]): Promise<BoundRefreshSummary> {
  const result = await runRepositoryBatch(repos, { force: true, trigger: 'bound' })
  return { refreshed: result.refreshed, updated: result.updated, errors: result.errors }
}

/**
 * 源文档失败的报错文案：`<昵称>: <原因>`。
 *
 * **必须与回退判定用同一个昵称取值**。`shouldFallbackToBuiltin` 靠
 * 「错误文案是否以官方源昵称开头」判断失败是否覆盖到官方根源。两边都用
 * `pickI18n(nickname_i18n, nickname)`（并通过同一个 locale），前缀才能对上；
 * 如果一边用翻译值、另一边用恒为 zh-CN 的镜像字段，繁体/英文界面下回退
 * 会被静默跳过 —— 正是用户报的「报错了，但没有回退」。
 */
function sourceDisplayName(source: RepositorySource): string {
  return pickI18n(source.nickname_i18n, source.nickname)
}

/**
 * 把「来源名 + 失败原因」拼成一条可展示的错误。
 *
 * 收口点在这里是因为三处调用（单仓刷新、源链刷新、失败回退）都是同一个
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
  if (visited.has(sourceId)) return []
  visited.add(sourceId)
  const { reloadConfig, sources, region } = store()
  const source = sources.value.find((entry) => entry.id === sourceId)
  if (!source) return []

  /* 源级标志也走共享状态机：地图侧用的是同一套写入规则 */
  const importErrors = await state.runSource(sourceId, async () => {
    try {
      const result = await api.importRepositorySource(source.id, source.url)
      return result.access_error ? [sourceError(source, result.access_error)] : []
    } catch (reason) {
      return [sourceError(source, reason instanceof Error ? reason.message : String(reason))]
    }
  })
  // 父源没读出来时它的子源无从谈起：直接返回，别拿旧 `sources` 递归
  if (importErrors.length) return importErrors

  await reloadConfig()
  // 子源同样按地区过滤：父源是通用的，但它可以声明地区专属子源
  const children = sources.value.filter(
    (entry) => entry.parent_id === sourceId && sourceMatchesRegion(entry, region.value),
  )
  if (!children.length) return []
  const childErrors = await Promise.all(children.map((child) => refreshSourceChain(child.id, visited)))
  return childErrors.flat()
}

/**
 * 完整刷新时，符合条件则尝试从内置快照加载官方源树。
 *
 * 判定不区分网络错误和解析错误，只检查错误与已得到的仓库：
 *   · 必须存在官方根源（`OFFICIAL_INDEX_ID`），且它在本次失败名单里；
 *   · 当前官方源树下没有任何仓库 —— 只要有一个子源成功带回仓库，
 *     就说明在线可用，不能再用落后几版的内置内容覆盖它；
 *   · 只对官方根调用一次（`builtinFallback: true`），它内部会把整个
 *     内置索引树（含子源）一起展开。
 *
 * **必须继续递归子源**：内置索引 `mods.toml` 自己**没有仓库**，
 * 它只声明子源（`mods/mods-azhe-hostutilities.toml` 等），仓库都在子源
 * 文件里。只应用索引不足以得到仓库，必须继续递归子源；子源仍可能分别失败。
 * 回退复用 refreshSourceChain，让在线和内置流程的子源处理保持一致。
 *
 * 返回 `{ fallbackErrors, hasFallenBack }`：
 *   · `fallbackErrors` 用于向用户报告失败原因（成功时为空）；
 *   · `hasFallenBack` 表示这次是否真的发送了内置回退请求。调用方必须用它
 *     （而不是「错误是否为空」）来决定要不要重读配置 —— 回退成功恰恰是
 *     「没有错误」的情况。
 */
async function fallbackToBuiltinIfOffline(
  sourceErrors: string[],
): Promise<{ fallbackErrors: string[]; hasFallenBack: boolean }> {
  const { sources, repositories, reloadConfig } = store()
  /*
   * 传当前界面语言：判定里的昵称前缀必须和 `sourceError` 生成时用的是
   * 同一个值，否则繁体/英文界面下前缀不匹配、回退被静默跳过。
   */
  if (!shouldFallbackToBuiltin(sources.value, repositories.value, sourceErrors, locale.value)) {
    return { fallbackErrors: [], hasFallenBack: false }
  }

  const officialRoot = sources.value.find((source) => source.id === OFFICIAL_INDEX_ID)
  if (!officialRoot) return { fallbackErrors: [], hasFallenBack: false }

  const errors: string[] = []
  const rootErrors = await state.runSource(officialRoot.id, async () => {
    try {
      const result = await api.importRepositorySource(officialRoot.id, officialRoot.url, true)
      return result.access_error ? [sourceError(officialRoot, result.access_error)] : []
    } catch (reason) {
      return [sourceError(officialRoot, reason instanceof Error ? reason.message : String(reason))]
    }
  })
  // 索引本身都没读出来：后面的子源无从谈起
  if (rootErrors.length) return { fallbackErrors: [...errors, ...rootErrors], hasFallenBack: true }

  /*
   * 索引已写入配置，子源现在才出现在 `sources` 里。
   * 用和在线流程完全相同的 `refreshSourceChain` 展开它们 ——
   * 内置子源地址是相对路径，后端 `SourceLocation` 会解析成打包资源，
   * 不需要任何「内置专用」分支。
   *
   * 传空的 visited：根源刚刚已经被单独应用过，这里只需要它的后代。
   */
  await reloadConfig()
  const childSources = sources.value.filter((entry) => entry.parent_id === officialRoot.id)
  const childErrors = await mapWithConcurrency(
    childSources,
    REFRESH_CONCURRENCY,
    (child) => refreshSourceChain(child.id, new Set([officialRoot.id])),
  )
  errors.push(...childErrors.flat())
  return { fallbackErrors: errors, hasFallenBack: true }
}

/**
 * 刷新全部源文档（根源并行，子源递归）。
 *
 * 只刷新当前地区适用的源：international 源在中国大陆既不展示，
 * 也不该被拉取——否则会拖慢刷新，失败时还会弹出一条用户根本无法处理的错误。
 */
async function refreshSourceDocuments(): Promise<string[]> {
  const { sources, region } = store()
  const refreshable = sources.value.filter(
    (source) => !COMMUNITY_SOURCE_IDS.has(source.id) && sourceMatchesRegion(source, region.value),
  )
  const known = new Set(refreshable.map((source) => source.id))
  // 父源已被移除的孤儿当作根源，避免整条链都不刷新
  const roots = refreshable.filter((source) => !source.parent_id || !known.has(source.parent_id))
  if (!roots.length) return []
  const visited = new Set<string>()
  /*
   * 根源也必须限并发：内置索引树有 8 个子源，`Promise.all` 会同时发 8 个
   * 文档请求，加上递归子源实际并发更高 —— 匿名 API 额度就是这么被磨掉的。
   * 限成 4 组后整条链（含递归子源调用）同时最多 4 个文档在飞。
   */
  const results = await mapWithConcurrency(
    roots,
    REFRESH_CONCURRENCY,
    (source) => refreshSourceChain(source.id, visited),
  )
  return results.flat()
}

/**
 * 社区仓库发现：并行搜索 GitHub 与 Gitee，把结果合并进配置。
 *
 * 只在官方源树刷新完毕之后调用（由 `refreshEverything` 保证顺序）：
 * 后端用官方源树判断同坐标去重，官方条目还没落地时会误判为「社区新增」。
 *
 * 两个平台并行：GitHub 走 Search API，Gitee 走独立 WebView 验证，互不排队。
 * 任一平台失败只收集错误 —— 另一平台的结果照常保留，整批刷新继续。
 * 搜索本身有频率限制与冷却（后端 rate gate），反复失败不会拖垮应用。
 *
 * @param applySnapshot 用返回的完整配置替换 Dashboard 快照（不再多发一次 get_config）
 */
async function discoverCommunityRepositories(
  applySnapshot: (config: PublicConfig) => void,
): Promise<string[]> {
  const providers: Provider[] = ['github', 'gitee']
  if (store().config.value?.gitcode_token_set) providers.push('gitcode')
  const results = await Promise.all(providers.map(async (provider) => {
    try {
      const merged = await api.refreshCommunityRepositories(provider)
      // 两个平台各自返回完整配置：后返回的已经包含先返回的合并结果，
      // 所以每次都整体替换即可，不需要自己拼。
      applySnapshot(merged)
      return null
    } catch (reason) {
      const label = providerLabel(provider)
      /*
       * 原因来自后端，可能是 tag。它作为**参数值**传进文案，vue-i18n 只会
       * 原样插入，不会二次翻译 —— 所以必须先在这里翻译。
       * `provider` 是产品名（GitHub / Gitee），各语言都保持原样，只翻外壳。
       */
      const message = backendMessageOf(reason)
      return t('modsOnline.communityRepositoryError', { provider: label, reason: message })
    }
  }))
  return results.filter((error): error is string => error !== null)
}

/**
 * 完整刷新：源文档（父→子）→ 全部启用仓库的 Release（并发 4）。
 * 返回失败清单；调用方决定是否提示。
 */
async function refreshEverything(): Promise<string[]> {
  if (refreshingAll.value) return []
  state.beginBatch('full')
  /*
   * 从清空源缓存到列表填回来，按仓库 id 的状态一个都匹配不上：这段空窗期
   * 由 `entryStateUnavailable` 兜住卡片标签。Release 阶段开始前必须关掉。
   */
  state.setEntryStateUnavailable(true)
  lastErrors.value = []
  try {
    const { load, reloadConfig, applyConfigSnapshot, repositories, sources, region, platform } = store()
    // 启动时 Dashboard 可能还没读过：先读一次才有源和仓库可刷
    if (!sources.value.length && !repositories.value.length) await load()
    /*
     * 先把旧缓存清掉，再拉取权威文档。
     *
     * 不这样做的话，页面会先用配置里上一轮的仓库列表渲染一遍，等 raw 文档
     * 回来再整体替换 —— 用户看到的是「旧列表一闪、随后跳变」，而且旧条目在
     * 新文档到达前是可点的（可能装到已经下线的仓库）。
     *
     * 清空只作用于本次要刷的源及其后代，社区源不受影响（它们由搜索管理）。
     * 后端返回清理后的配置，这里直接写回 Dashboard，列表当场变空。
     */
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
    const sourceErrors = await refreshSourceDocuments()
    // 源文档阶段用的是 reloadConfig（只更新配置）；
    // 这里再读一次配置拿到最终仓库列表，Release 阶段结束时会做一次完整 load。
    await reloadConfig()
    /*
     * 官方在线源全军覆没时改用内置快照。
     *
     * 条件（三条同时满足才回退，任一不满足都不回退）：
     *   1. 官方源本身启用且属于官方树（`DEFAULT_INDEX_ID` 及其子源）；
     *   2. 本次源文档阶段确实失败过（sourceErrors 覆盖到官方树）；
     *   3. 当前官方源树下**一个仓库都没有**——部分子源成功、或子源可访问但
     *      没有仓库，都不算「全失败」，不能拿内置内容去覆盖真实结果。
     *
     * 内置快照是落后几个版本的官方源，只作为离线兜底；一旦回退成功，
     * 后续每次刷新仍先请求在线入口，不会固定在内置版本。
     */
    const { fallbackErrors, hasFallenBack } = await fallbackToBuiltinIfOffline(sourceErrors)
    /*
     * 回退成功时**也要**重读配置：内置仓库是在这次命令里写进配置的，
     * 只有重新读一次才会出现在页面上。
     *
     * 这里踩过一次：原先写成 `if (fallbackErrors.length) reloadConfig()`，
     * 而回退成功返回的是空数组（没有错误），于是重读被跳过 —— 结果是
     * 「回退请求发了、配置也写了，但页面上一个仓库都不显示」。
     * 用 hasFallenBack 表达「这次是否真的走了回退」，与「有没有报错」解耦。
     */
    if (hasFallenBack) await reloadConfig()
    /*
     * 社区仓库发现：官方源树**刷新完毕之后**才触发，且不阻塞后面的 Release 刷新。
     *
     * 顺序要求来自用户：社区搜索依赖官方源树来判断「哪些同坐标已被官方收录」，
     * 所以必须等官方源提交完成，否则会把官方已有的仓库当成社区新增；
     * 而两个平台互相独立，并行发起即可 —— GitHub 走 Search API、Gitee 走
     * 网页验证，互不排队。
     *
     * 结果由后端 `merge_results` 写进配置（含「被官方覆盖的社区副本置
     * enabled=false」的闪避），这里把返回的完整配置直接写回 Dashboard。
     * 任一平台失败只记录，不影响另一平台，也不影响整批刷新。
     */
    const communityErrors = await discoverCommunityRepositories(applyConfigSnapshot)
    // 配置已经写回（或发现失败也不该让地图刷新无限等），地图源树可以读了
    markCommunityDiscovered()

    /*
     * 已安装优先：先把本地装过的 MOD 所绑定的仓库排到队首。
     *
     * 首次启动与「刷新全部仓库源」都走这里，所以两条路径行为一致：
     * 用户关心的是自己装的仓库有没有更新，不该排在几十个未安装仓库
     * 后面慢慢等。
     *
     * 不可用的仓库整条跳过，判定与安装页展示完全一致
     * （见 `repositoryIsAvailable`）：
     *   - `enabled=false`：被用户关闭，或被官方仓库源覆盖的社区副本；
     *   - 地区不适用：例如 international 仓库在中国大陆。
     * 这两类既不显示也不刷新，前端不会为它们发出任何请求。
     */
    const available = repositories.value.filter((repo) =>
      repositoryIsAvailable(repo, sources.value, region.value, platform.value))
    /*
     * trigger 传 `single`：整批的结果由下面统一写一条 `full`，
     * 否则这里会先把「上次检查」覆盖成半批数据（源文档与社区阶段的错误还没并进来）。
     * 已安装优先已下沉到 `runRepositoryBatch`，不必在这里再排一次。
     */
    /*
     * 仓库列表已由 `reloadConfig` 填回来，条目级状态从这里接管。
     *
     * 关掉批次兜底是「按仓库粒度返回」的前提：留着它，整批标志会把每张卡
     * 都按在「刷新中」，直到最后一个仓库刷完才一起松开 —— 表现就是用户报的
     * 「结果一批一起回来」。
     */
    state.setEntryStateUnavailable(false)
    const release = await runRepositoryBatch(available, { trigger: 'single' })
    /*
     * 把线上依赖声明同步进本地清单（MOD 安装清单 + 地图依赖清单）。
     *
     * 依赖声明只存在于仓库源文档里：源下线、文档改版或用户删了自定义源之后，
     * 本地已装 MOD 的依赖信息就只剩安装那一刻的快照。这里按刚刷新到的配置
     * 重写一遍，保证「在线与本地不一致时以在线为准」。
     *
     * 失败吞掉是刻意的：快照是展示用的兜底信息，它同步不上不该让用户看到
     * 一条整批刷新失败的提示 —— 刷新本身已经成功了。
     */
    await api.syncDependencySnapshots().catch(() => undefined)
    lastErrors.value = [...sourceErrors, ...fallbackErrors, ...communityErrors, ...release.errors]
    /*
     * 整批刷新跨了四个阶段（源文档、内置回退、社区发现、全部条目），
     * 错误分散在四处，只有这里知道怎么合并 —— 所以结果由本函数自己写，
     * 而不是让 `runRepositoryBatch` 把「只看到最后一段」的结果写出去。
     */
    state.publishOutcome({
      trigger: 'full',
      refreshed: release.refreshed,
      updated: release.updated,
      errors: lastErrors.value,
      finishedAt: Date.now(),
    })
    return lastErrors.value
  } finally {
    // 发现阶段之前就抛错时也要兑现：地图启动刷新等在它后面，不能永远挂着
    markCommunityDiscovered()
    state.setEntryStateUnavailable(false)
    state.endBatch('full')
  }
}

/**
 * 「社区发现阶段已经结束」信号。
 *
 * 地图源树也在启动时刷新一次，而社区发现的地图源仓库是**写进配置**、由
 * 地图源树读的。两件事在启动时并发发起的话，地图那边很可能先跑完、读到的
 * 还是不含社区仓库的旧配置 —— 首启就看不到社区地图，得等用户手动刷新地图页。
 *
 * 所以把「发现写完了」做成可等待的信号，让地图启动刷新挂在这里：只等社区
 * 搜索（几秒），不等后面的 Release 批次。
 */
let communityDiscovered: Promise<void> = Promise.resolve()
let settleCommunityDiscovered: () => void = () => {}

function resetCommunityDiscoverySignal() {
  communityDiscovered = new Promise<void>((resolve) => { settleCommunityDiscovered = resolve })
}

/** 等本轮启动刷新把社区发现写完（没在刷新时立刻兑现）。 */
function whenCommunityDiscovered(): Promise<void> {
  return communityDiscovered
}

function markCommunityDiscovered() {
  settleCommunityDiscovered()
}

/**
 * 安排启动刷新（安装器启动时由 `App.vue` 调用一次）。
 *
 * V2 的前端教程系统已整体裁剪，所以启动后立即刷新，没有门控。
 * `deferStartupRefresh()` / `releaseStartupRefresh()` 是当时留下的门控脚手架，
 * 目前没有任何调用方；如果确定不再恢复教程，可以连同 `deferred` 状态一起删。
 */
function requestStartupRefresh() {
  if (startupState !== 'idle') return
  startupState = 'running'
  resetCommunityDiscoverySignal()
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

export function useRepositoryRefresh() {
  return {
    refreshing, queued, sourceRefreshing, refreshingAll, entryStateUnavailable,
    checkingBound, anyRefreshing, repositoriesLoading,
    outcome, lastErrors, startupRefreshPending,
    refreshOne, refreshMany, refreshBoundRepositories,
    refreshSourceChain, refreshSourceDocuments, refreshEverything,
    requestStartupRefresh, deferStartupRefresh, releaseStartupRefresh,
    whenCommunityDiscovered,
  }
}
