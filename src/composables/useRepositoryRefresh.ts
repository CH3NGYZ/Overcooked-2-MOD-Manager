import { computed, ref } from 'vue'
import { api } from '../services/tauri'
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
 */

/** 社区仓库源由自动搜索管理，不参与文档刷新。 */
const COMMUNITY_SOURCE_IDS = new Set(['github-community', 'gitee-community'])
const RELEASE_CONCURRENCY = 4

/** 仓库 Release 正在刷新（按仓库 id）。 */
const refreshing = ref<Record<string, boolean>>({})
/** 源文档正在刷新（按源 id，含父源与子源）。 */
const sourceRefreshing = ref<Record<string, boolean>>({})
/** 整批刷新进行中。 */
const refreshingAll = ref(false)

/** 任一刷新活动：按钮 loading/禁用统一看它。 */
const anyRefreshing = computed(() =>
  refreshingAll.value
  || Object.values(refreshing.value).some(Boolean)
  || Object.values(sourceRefreshing.value).some(Boolean),
)

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
 * 因此这里不直接用 `refreshingAll`（它在源文档阶段就已经是 true，会持续
 * 几十秒），而是看 `repositoriesInFlight` 这个精确计数：`refreshMany`
 * 真正开始处理仓库队列时置数、整批结束时归零。
 *
 * 它是模块级单例派生，所以「启动时正在刷新」「在 MOD 管理页点了刷新」
 * 都会让 MOD 安装页的同一个按钮转起来 —— 与从哪个页面发起无关。
 */
const repositoriesInFlight = ref(0)
const repositoriesLoading = computed(() =>
  repositoriesInFlight.value > 0 || Object.values(refreshing.value).some(Boolean),
)

/** 最近一次整批刷新的失败信息（供页面决定是否提示）。 */
const lastErrors = ref<string[]>([])

/**
 * 启动刷新的门控状态：
 *   idle     → 还没安排（页面自行决定）
 *   deferred → 教程进行中，等教程结束
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
  return { load, reloadConfig, applyConfigSnapshot, sources, repositories, mods, region }
}

/**
 * 单仓刷新：拿到 Release 后只更新这一条仓库的缓存，成功返回 null，失败返回错误文案。
 *
 * 这里刻意不调用 load()：全量重读会重新扫描游戏目录和每个 DLL，
 * 在滚动时刷新表现为明显卡顿。单个仓库的结果局部替换即可，
 * 整批结束后再统一 load() 一次补齐其它派生状态（更新数、MOD 标签等）。
 */
async function refreshOne(repo: Repository): Promise<string | null> {
  if (refreshing.value[repo.id]) return null
  refreshing.value[repo.id] = true
  try {
    const releases = await api.refreshRepository(repo.id)
    applyRepositoryReleases(repo.id, releases)
    return null
  } catch (reason) {
    return `${repo.name}: ${reason instanceof Error ? reason.message : String(reason)}`
  } finally {
    refreshing.value[repo.id] = false
  }
}

/**
 * 把单个仓库的新 Release 写回 Dashboard 快照。
 *
 * dashboard 是 shallowRef：必须整体替换引用才会触发视图更新，
 * 但只重建「这一条仓库」，其余仓库、MOD 列表和游戏状态沿用原对象，
 * 所以不会触发整页数据替换带来的重绘卡顿。
 */
function applyRepositoryReleases(repositoryId: string, releases: Repository['cached_releases']) {
  const { dashboard } = useDashboard()
  const current = dashboard.value
  if (!current) return
  const config = current.config
  const index = config.repositories.findIndex((repo) => repo.id === repositoryId)
  if (index < 0) return
  const repositories = config.repositories.slice()
  repositories[index] = { ...repositories[index], cached_releases: releases, last_error: undefined }
  dashboard.value = {
    ...current,
    config: { ...config, repositories },
  }
}

/**
 * 把「有本地已安装 MOD 的仓库」排到队列最前，其余保持原有顺序。
 * 实现见 `src/repositoryPriority.ts`（纯函数，便于直接测试）。
 */

/**
 * 限并发的批量刷新，「刷一个显示一个」由局部更新实现。
 *
 * 每个仓库完成时只替换它自己的 cached_releases（见 refreshOne），
 * 视图立刻显示该仓库的版本号和状态，其余仓库继续转圈；
 * 整批结束后才 load() 一次，补齐更新数、MOD 标签等派生状态。
 * 这样刷新过程中不会反复触发整份 Dashboard 重读（那会重新扫描游戏目录和 DLL，
 * 表现为页面滚动时明显卡一下）。
 */
async function refreshMany(repos: Repository[]) {
  // 仓库文件（RAW）模式没有 Release 可刷，直接跳过，不当成失败上报
  const queue = repos.filter((repo) => !refreshing.value[repo.id] && repo.download_mode !== 'repository_file')
  if (!queue.length) return [] as string[]
  /*
   * 进入仓库阶段：让「刷新按钮转不转」立刻变为真。
   * 从这里一直到 finally，队列里始终有仓库在处理（并发 4 个 worker），
   * 所以计数量 1 就够；结束时归零，图标随即停止。
   */
  repositoriesInFlight.value += 1
  const errors: string[] = []
  let index = 0
  const workers = Array.from({ length: Math.min(RELEASE_CONCURRENCY, queue.length) }, async () => {
    while (index < queue.length) {
      const repo = queue[index]
      index += 1
      const error = await refreshOne(repo)
      if (error) errors.push(error)
    }
  })
  try {
    await Promise.all(workers)
    await store().load()
  } finally {
    /*
     * 无论成功、失败还是抛错都归零：图标必须停，
     * 不能因为某个仓库抛异常就一直转下去。
     */
    repositoriesInFlight.value = Math.max(0, repositoriesInFlight.value - 1)
  }
  return errors
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

  sourceRefreshing.value[sourceId] = true
  try {
    const result = await api.importRepositorySource(source.id, source.url)
    if (result.access_error) return [`${source.nickname}: ${result.access_error}`]
  } catch (reason) {
    return [`${source.nickname}: ${reason instanceof Error ? reason.message : String(reason)}`]
  } finally {
    sourceRefreshing.value[sourceId] = false
  }

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
 * 官方在线源全部失败时，改从内置快照加载官方源树。
 *
 * 只在「真正离线 / 官方入口不可达」时兜底，判断刻意保守：
 *   · 必须存在官方根源（`official-index`），且它在本次失败名单里；
 *   · 当前官方源树下没有任何仓库 —— 只要有一个子源成功带回仓库，
 *     就说明在线可用，不能再用落后几版的内置内容覆盖它；
 *   · 只对官方根调用一次（`builtinFallback: true`），它内部会把整个
 *     内置索引树（含子源）一起展开。
 *
 * **必须继续递归子源**：内置索引 `repositories.toml` 自己**没有仓库**，
 * 它只声明子源（`repositories-azhe-hostutilities.toml` 等），仓库都在子源
 * 文件里。只加载索引会得到「回退成功、子源已写入、但一个仓库都没有」——
 * 这正是用户报的「报错之后没回退到内置仓库源，列表全空」。
 * 之前这里只调了一次 `importRepositorySource` 就返回，漏掉了与在线流程
 * 同一套的 `refreshSourceChain` 递归。现在回退后复用同一个函数展开子源，
 * 两条路径的子源处理不再分叉。
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
  if (!shouldFallbackToBuiltin(sources.value, repositories.value, sourceErrors)) {
    return { fallbackErrors: [], hasFallenBack: false }
  }

  const officialRoot = sources.value.find((source) => source.id === OFFICIAL_INDEX_ID)
  if (!officialRoot) return { fallbackErrors: [], hasFallenBack: false }

  const errors: string[] = []
  sourceRefreshing.value[officialRoot.id] = true
  try {
    const result = await api.importRepositorySource(officialRoot.id, officialRoot.url, true)
    if (result.access_error) {
      errors.push(`${officialRoot.nickname}: ${result.access_error}`)
      // 索引本身都没读出来：后面的子源无从谈起
      return { fallbackErrors: errors, hasFallenBack: true }
    }
  } catch (reason) {
    errors.push(`${officialRoot.nickname}: ${reason instanceof Error ? reason.message : String(reason)}`)
    return { fallbackErrors: errors, hasFallenBack: true }
  } finally {
    sourceRefreshing.value[officialRoot.id] = false
  }

  /*
   * 索引已写入配置，子源现在才出现在 `sources` 里。
   * 用和在线流程完全相同的 `refreshSourceChain` 展开它们 ——
   * 内置子源地址是相对路径，后端 `SourceLocation` 会解析成打包资源，
   * 不需要任何「内置专用」分支。
   *
   * 传空的 visited：根源刚刚已经被单独应用过，这里只需要它的后代。
   */
  await reloadConfig()
  const childErrors = await Promise.all(
    sources.value
      .filter((entry) => entry.parent_id === officialRoot.id)
      .map((child) => refreshSourceChain(child.id, new Set([officialRoot.id]))),
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
  const results = await Promise.all(roots.map((source) => refreshSourceChain(source.id, visited)))
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
  const results = await Promise.all(providers.map(async (provider) => {
    try {
      const merged = await api.refreshCommunityRepositories(provider)
      // 两个平台各自返回完整配置：后返回的已经包含先返回的合并结果，
      // 所以每次都整体替换即可，不需要自己拼。
      applySnapshot(merged)
      return null
    } catch (reason) {
      const label = provider === 'github' ? 'GitHub' : 'Gitee'
      const message = reason instanceof Error ? reason.message : String(reason)
      return `${label} 社区仓库：${message}`
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
  refreshingAll.value = true
  lastErrors.value = []
  try {
    const { load, reloadConfig, applyConfigSnapshot, repositories, sources, mods, region } = store()
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
        lastErrors.value.push(reason instanceof Error ? reason.message : String(reason))
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
    const ordered = prioritizeInstalledRepositories(
      repositories.value.filter((repo) => repositoryIsAvailable(repo, sources.value, region.value)),
      mods.value,
    )
    const releaseErrors = await refreshMany(ordered)
    lastErrors.value = [...sourceErrors, ...fallbackErrors, ...communityErrors, ...releaseErrors]
    return lastErrors.value
  } finally {
    refreshingAll.value = false
  }
}

/**
 * 安排启动刷新（安装器启动时调用一次）。
 * 教程系统接入后：教程进行中先调用 `deferStartupRefresh()`，
 * 教程结束再调用 `releaseStartupRefresh()`，刷新会在那一刻才开始。
 */
function requestStartupRefresh() {
  if (startupState !== 'idle') return
  startupState = 'running'
  void refreshEverything().finally(() => { startupState = 'done' })
}

/** 教程开始时暂缓启动刷新。 */
function deferStartupRefresh() {
  if (startupState !== 'idle') return
  startupState = 'deferred'
}

/** 教程结束/跳过时释放之前的暂缓，并立刻执行首次刷新。 */
function releaseStartupRefresh() {
  if (startupState === 'deferred' || startupState === 'idle') requestStartupRefresh()
}

/** 是否还有待执行的启动刷新（页面可据此显示提示）。 */
const startupRefreshPending = computed(() => startupState === 'deferred' || startupState === 'idle')

export function useRepositoryRefresh() {
  return {
    refreshing, sourceRefreshing, refreshingAll, anyRefreshing, repositoriesLoading,
    lastErrors, startupRefreshPending,
    refreshOne, refreshMany, refreshSourceChain, refreshSourceDocuments, refreshEverything,
    requestStartupRefresh, deferStartupRefresh, releaseStartupRefresh,
  }
}
