import { computed, ref } from 'vue'
import { api } from '../services/tauri'
import { useDashboard } from './useDashboardStore'
import { prioritizeInstalledRepositories } from '../repositoryPriority'
import { repositoryIsAvailable, sourceMatchesRegion } from '../repositoryAvailability'
import type { Repository, RepositorySource } from '../types'

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
  const { load, reloadConfig, config, mods } = useDashboard()
  const sources = computed<RepositorySource[]>(() => config.value?.repository_sources ?? [])
  const repositories = computed<Repository[]>(() => config.value?.repositories ?? [])
  /** 当前地区决定哪些源/仓库真实存在；空值按「非中国大陆」处理，与后端一致。 */
  const region = computed(() => config.value?.region)
  return { load, reloadConfig, sources, repositories, mods, region }
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
  await Promise.all(workers)
  await store().load()
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
 * 完整刷新：源文档（父→子）→ 全部启用仓库的 Release（并发 4）。
 * 返回失败清单；调用方决定是否提示。
 */
async function refreshEverything(): Promise<string[]> {
  if (refreshingAll.value) return []
  refreshingAll.value = true
  lastErrors.value = []
  try {
    const { load, reloadConfig, repositories, sources, mods, region } = store()
    // 启动时 Dashboard 可能还没读过：先读一次才有源和仓库可刷
    if (!sources.value.length && !repositories.value.length) await load()
    const sourceErrors = await refreshSourceDocuments()
    // 源文档阶段用的是 reloadConfig（只更新配置）；
    // 这里再读一次配置拿到最终仓库列表，Release 阶段结束时会做一次完整 load。
    await reloadConfig()
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
    lastErrors.value = [...sourceErrors, ...releaseErrors]
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
    refreshing, sourceRefreshing, refreshingAll, anyRefreshing,
    lastErrors, startupRefreshPending,
    refreshOne, refreshMany, refreshSourceChain, refreshSourceDocuments, refreshEverything,
    requestStartupRefresh, deferStartupRefresh, releaseStartupRefresh,
  }
}
