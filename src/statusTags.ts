import type { InstalledMod, InstalledModStatusLabel, Repository } from './types'
import { compareVersions, isPrereleaseVersion, modsForRepository, repositoryForMod } from './modStatus.ts'

/**
 * MOD 状态标签：两页共用同一套「文案 + 语义色」，避免两边各写一份。
 *
 * - MOD 管理页：直接渲染后端 `labels`（已含渠道、Tag 绑定、重复副本判定）。
 * - MOD 安装页：按「已安装版本 vs 仓库最新 Release」比较得出。
 *
 * 颜色在 `src/styles/global.css` 的 `.state-tag--*` 里定义。
 */
/**
 * 状态色调 —— 按「严重程度」五档划分。
 *
 * 这是全站唯一的严重度色阶，所有标签都归入其中一档：
 *   accent    正常   主题色   最新版、已安装、部分启用
 *   mild      轻微   对比色弱 低版本副本
 *   moderate  中度   对比色中 可更新、预发布
 *   severe    重度   对比色强 重复（同 GUID 冲突）
 *   critical  严重   固定红   来源未知、仓库缺失/不可用、加载警告
 *   muted     中性   灰       未安装、未绑定、已禁用、版本未知
 *
 * 旧值（ok/warn/idle/danger/violet）保留在类型里只为兼容旧快照，
 * 新代码一律使用五档名称。
 */
export type StatusTone =
  | 'accent' | 'mild' | 'moderate' | 'severe' | 'critical' | 'muted'
  /** @deprecated 用 accent */
  | 'ok' | 'idle'
  /** @deprecated 用 moderate */
  | 'warn'
  /** @deprecated 用 critical */
  | 'danger'
  /** @deprecated 用 mild */
  | 'violet'

export interface StatusTag {
  key: string
  text: string
  tone: StatusTone
}

/** 后端标签 → 文案与颜色。 */
export const STATUS_META: Record<string, { text: string; tone: StatusTone }> = {
  update_available: { text: '可更新', tone: 'moderate' },
  latest: { text: '最新版', tone: 'accent' },
  /** 已安装的是预发布版本：与「可更新」同档，因为都需要用户注意。 */
  prerelease: { text: '预发布', tone: 'moderate' },
  not_release_asset: { text: '已安装', tone: 'accent' },
  unbound: { text: '未绑定', tone: 'muted' },
  bound_repository_missing: { text: '仓库缺失', tone: 'critical' },
  /**
   * 绑定仓库的 Release 抓取失败。
   *
   * 与 MOD 安装页的 `refresh_failed` 是同一件事（这个仓库这次没刷出来），
   * 所以文案统一为「仓库刷新失败」、等级统一为 moderate：
   * 需要用户注意并重试，但不像同 GUID 冲突、加载失败那样已经破坏了本地状态。
   */
  repository_unavailable: { text: '仓库刷新失败', tone: 'moderate' },
  /**
   * 正在检查：绑定的仓库正在抓取 Release，结论还没出来。
   *
   * 与 `unknown`（版本未知）语义不同：unknown 是「查完了也判断不出来」，
   * checking 是「还在查」。混用会让刷新过程显示成「版本未知」，
   * 用户会误以为已经出结果了。中性灰，因为此刻没有结论、也不需要动作。
   */
  checking: { text: '检测中', tone: 'muted' },
  unknown: { text: '版本未知', tone: 'muted' },
  duplicate: { text: '重复', tone: 'severe' },
  duplicate_lower_version: { text: '低版本', tone: 'mild' },
  load_warning: { text: '加载警告', tone: 'critical' },
  disabled: { text: '已禁用', tone: 'muted' },
  /** 包内的纯依赖 DLL：读不出 BepInPlugin 元数据，只作为依赖展示。 */
  dependency: { text: '依赖', tone: 'muted' },
  /** 仓库文件（RAW）已安装：与「最新版」同档，都用主题色。 */
  installed: { text: '已安装', tone: 'accent' },
  // MOD 安装页专用：仓库维度而不是已安装 MOD 维度
  /** 未安装一律灰色，不区分 Release / RAW。 */
  not_installed: { text: '未安装', tone: 'muted' },
  /** RAW（仓库文件）仓库未安装时也显示「未安装」，不再显示「仓库文件」。 */
  not_installed_raw: { text: '未安装', tone: 'muted' },
  not_refreshed: { text: '未刷新', tone: 'muted' },
  /** 与已安装 MOD 的 `repository_unavailable` 同义，文案与等级保持一致。 */
  refresh_failed: { text: '仓库刷新失败', tone: 'moderate' },
  repository_disabled: { text: '未启用', tone: 'muted' },
}

/**
 * 已安装 MOD 的标签列表。
 *
 * 后端 `labels` 为空时（旧快照或测试数据）退回 `status` 单标签。
 * 已安装的是预发布版本时，额外在最前面插一个「预发布」标签
 * （它不参与后端状态判定，只表达「这个版本是 pre-release」）。
 */
/**
 * 版本结论类标签：一个 MOD 只能有一个。
 *
 * 「最新版 / 可更新 / 已安装 / 版本未知 / 仓库缺失 / 仓库不可用 / 未绑定」
 * 描述的是同一件事——这个 MOD 相对于仓库的版本结论，天然互斥。
 * 它们同时出现就是 bug（例如重算更新状态后残留旧的「仓库不可用」）。
 *
 * 后端 `refresh_status_labels` 用 if/else 保证互斥；前端 `effectiveMods`
 * 重算时必须用 `replaceVersionConclusion()` 而不是自己拼接，否则会
 * 留下旧结论，出现「最新版 + 仓库不可用」这类矛盾组合。
 */
export const VERSION_CONCLUSION_LABELS = [
  'latest',
  'update_available',
  'not_release_asset',
  'installed',
  'checking',
  'unknown',
  'unbound',
  'bound_repository_missing',
  'repository_unavailable',
  'refresh_failed',
  'not_refreshed',
  'not_installed',
  'not_installed_raw',
  'repository_disabled',
] as const

const VERSION_CONCLUSION_SET = new Set<string>(VERSION_CONCLUSION_LABELS)

/** 该标签是否属于「版本结论」这一互斥组。 */
export function isVersionConclusion(label: string): boolean {
  return VERSION_CONCLUSION_SET.has(label)
}

/**
 * 用新的版本结论替换旧的，保留所有非结论类标签（重复、低版本、加载警告、
 * 已禁用、依赖、预发布等）。
 *
 * @param labels 当前标签列表
 * @param conclusion 新的结论标签；传 `null` 表示只剥离旧结论、不追加新的
 */
export function replaceVersionConclusion(
  labels: readonly string[] | undefined,
  conclusion: string | null,
): string[] {
  const kept = (labels ?? []).filter((label) => !isVersionConclusion(label))
  return conclusion ? [conclusion, ...kept] : kept
}

export function installedModTags(mod: InstalledMod): StatusTag[] {
  const labels: string[] = mod.labels?.length ? mod.labels : [mod.status]
  const tags = labels.map((label: InstalledModStatusLabel | string) => {
    const meta = STATUS_META[label]
    return { key: label, text: meta?.text ?? label, tone: meta?.tone ?? 'muted' }
  })
  /*
   * 预发布判定取「元数据标记」或「版本号带 - 后缀」：
   * 作者没勾 prerelease 但版本叫 0.4.0-beta 时，同样要提示。
   */
  const prerelease = isPrereleaseVersion(localVersion(mod), mod.prerelease)
  if (prerelease && !tags.some((tag) => tag.key === 'prerelease')) {
    const meta = STATUS_META.prerelease
    tags.unshift({ key: 'prerelease', text: meta.text, tone: meta.tone })
  }
  return tags
}

/** 本地已装版本：优先安装清单记录的 Release tag，其次插件元数据版本。 */
export function localVersion(mod: InstalledMod): string | undefined {
  return mod.repository_binding?.tag || mod.version
}

/**
 * 状态聚合：把一组 MOD 的标签按「轻重缓急」汇总成带数量的标签。
 *
 * MOD 管理页的页头聚合与 MOD 包标题共用这一份实现 —— 两处都是
 * 「这一堆 MOD 里有多少个处于某状态」，各写一份迟早会漂移。
 *
 * 排序 = 语义 tone 的严重度（越需要注意越靠前），同档内再按固定业务顺序。
 * 数量为 0 的状态不返回，调用方不需要再过滤。
 */
const TONE_PRIORITY: Record<string, number> = {
  critical: 0,  // 严重：加载警告、仓库缺失
  severe: 1,    // 重度：重复
  moderate: 2,  // 中度：可更新、仓库刷新失败、预发布
  mild: 3,      // 轻微：低版本
  muted: 4,     // 中性：检测中、版本未知、未绑定、已禁用
  accent: 5,    // 正常：最新版、已安装
}

/** 同档内的稳定排序（越靠前越需要动作）。 */
const WITHIN_TONE_ORDER: string[] = [
  'load_warning',
  'bound_repository_missing',
  'repository_unavailable',
  'duplicate',
  'update_available',
  'prerelease',
  'refresh_failed',
  'duplicate_lower_version',
  'checking',
  'unknown',
  'unbound',
  'disabled',
  'dependency',
  'not_release_asset',
  'installed',
  'latest',
]

export interface StatusSummary {
  key: string
  text: string
  tone: StatusTone
  count: number
}

export interface StatusSummaryOptions {
  /** 不计入统计的标签（例如包标题自己已经画了「已禁用」/「部分启用」）。 */
  exclude?: readonly string[]
  /** 是否统计纯依赖 DLL。默认不统计：它不是 MOD，没有版本语义。 */
  includeDependencies?: boolean
}

export function summarizeStatuses(
  mods: readonly InstalledMod[],
  options: StatusSummaryOptions = {},
): StatusSummary[] {
  const excluded = new Set(options.exclude ?? [])
  const counted = options.includeDependencies
    ? mods
    : mods.filter((mod) => !mod.is_dependency)
  const countOf = (key: string) =>
    counted.filter((mod) => (mod.labels as string[] | undefined)?.includes(key)).length

  const entries: Array<StatusSummary & { index: number }> = []
  WITHIN_TONE_ORDER.forEach((key, index) => {
    if (excluded.has(key)) return
    const meta = STATUS_META[key]
    if (!meta) return
    const count = countOf(key)
    if (count > 0) entries.push({ key, text: meta.text, tone: meta.tone, count, index })
  })

  return entries
    .sort((a, b) => {
      const byTone = (TONE_PRIORITY[a.tone] ?? 9) - (TONE_PRIORITY[b.tone] ?? 9)
      return byTone !== 0 ? byTone : a.index - b.index
    })
    .map(({ index: _index, ...entry }) => entry)
}

/**
 * 仓库维度的状态：用「已安装 MOD 的最高版本」与「仓库最新 Release」比较。
 * 后端按真实安装清单判定（含渠道、Tag 绑定、重复副本），能对上这个仓库时优先采信；
 * 只在后端绑定的不是这个仓库时，才退回本地版本比较。
 */
/** 仓库卡片主按钮的动作：安装 / 重装 / 更新。 */
export type InstallAction = 'install' | 'reinstall' | 'update'

/**
 * 由仓库状态推导主按钮。
 *
 * 规则（用户确认）：
 *   常规仓库：未安装 → 安装；已安装且最新 → 重装；已安装且有更新 → 更新。
 *   仓库文件：未安装 → 安装；已安装 → 重装（没有版本比较的意义）。
 *   未刷新 / 刷新失败 / 未启用时按钮仍可点，按「安装」处理，
 *   交给后端在真正安装前再校验一次版本。
 */
export function repositoryInstallAction(status: StatusTag, repo: Repository): InstallAction {
  if (repo.download_mode === 'repository_file') {
    return status.key === 'installed' ? 'reinstall' : 'install'
  }
  switch (status.key) {
    case 'latest':
      return 'reinstall'
    case 'update_available':
      return 'update'
    default:
      return 'install'
  }
}

export function repositoryStatus(repo: Repository, installedMods: InstalledMod[]): StatusTag {
  const tag = (key: keyof typeof STATUS_META): StatusTag => ({
    key,
    text: STATUS_META[key].text,
    tone: STATUS_META[key].tone,
  })

  // RAW（仓库文件）仓库没有 Release 版本，只有「装没装」两种状态
  const isRaw = repo.download_mode === 'repository_file'
  if (!isRaw) {
    /*
     * 隐藏的仓库（用户主动关掉，或被官方源覆盖的社区副本）不刷新，
     * 缓存里的 Release 不可信，直接报「未启用」而不是拿旧结论下判断。
     */
    if (!repo.enabled) return tag('repository_disabled')
    if (repo.last_error) return tag('refresh_failed')
  }

  const release = repo.cached_releases[0]
  const matches = modsForRepository(installedMods, repo)
  const installed = matches.reduce<InstalledMod | undefined>((best, mod) => {
    if (!best) return mod
    const compared = compareVersions(localVersion(mod), localVersion(best))
    return compared !== null && compared > 0 ? mod : best
  }, undefined)

  if (!installed) return tag(isRaw ? 'not_installed_raw' : release ? 'not_installed' : 'not_refreshed')
  // RAW 已安装：没有版本比较的意义，直接显示「已安装」
  if (isRaw) return tag('installed')

  const boundHere = installed.repository_id === repo.id || !!installed.repository_ids?.includes(repo.id)
  if (boundHere && installed.labels?.length) {
    if (installed.update_available) return tag('update_available')
    if (installed.labels.includes('latest')) return tag('latest')
  }

  if (!release) return tag('unknown')
  const compared = compareVersions(release.version || release.tag_name, localVersion(installed))
  if (compared === null) return tag('unknown')
  return compared > 0 ? tag('update_available') : tag('latest')
}

/**
 * 用当前仓库缓存重算一个已安装 MOD 的更新状态。
 *
 * 为什么需要它：MOD 的 `labels` / `update_available` 是后端扫描时算好的，
 * 而仓库 Release 是异步刷新回来的。刷新期间 Dashboard 不重读（那会重扫磁盘），
 * 所以标签会一直停在「版本未知」，直到整批结束才更新。
 *
 * 这里用同一套判定规则在前端重算，让「刷完一个仓库 → 该仓库绑定的 MOD 立刻
 * 显示最新版/可更新」，不需要任何 IPC，也不碰磁盘。
 *
 * 返回 null 表示无法判断（没绑定仓库、仓库没有 Release、版本不可比较），
 * 调用方应保留后端给的原值。
 */
export function recomputeModUpdate(
  mod: InstalledMod,
  repositories: Repository[],
): { update_available: boolean; latest: boolean } | null {
  // 纯依赖不是 MOD，不参与更新判定
  if (mod.is_dependency) return null
  const repo = repositoryForMod(repositories, mod)
  if (!repo || repo.last_error) return null
  // 仓库文件模式没有版本比较的意义
  if (repo.download_mode === 'repository_file') return null
  const release = repo.cached_releases[0]
  if (!release) return null
  const compared = compareVersions(release.version || release.tag_name, localVersion(mod))
  if (compared === null) return null
  return compared > 0
    ? { update_available: true, latest: false }
    : { update_available: false, latest: true }
}
