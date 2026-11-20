import type { InstalledMod, InstalledModStatusLabel, ModRepository } from './types'
import { compareVersions, isPrereleaseVersion, localVersion, modsForRepository, repositoryForMod } from './modStatus.ts'
import { DEPENDENCY_ISSUE_TEXT } from './modRelations.ts'

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
  update_available: { text: 'tools.update', tone: 'moderate' },
  latest: { text: 'modsLocal.latest', tone: 'accent' },
  /** 已安装的是预发布版本：与「可更新」同档，因为都需要用户注意。 */
  prerelease: { text: 'modsOnline.preRelease', tone: 'moderate' },
  not_release_asset: { text: 'modsLocal.installed', tone: 'accent' },
  unbound: { text: 'modsLocal.unbound', tone: 'muted' },
  bound_repository_missing: { text: 'modsLocal.repositoryMissing', tone: 'critical' },
  /**
   * 绑定仓库的 Release 抓取失败。
   *
   * 与 MOD 安装页的 `refresh_failed` 是同一件事（这个仓库这次没刷出来），
   * 所以文案统一为「仓库刷新失败」、等级统一为 moderate：
   * 需要用户注意并重试，但不像同 GUID 冲突、加载失败那样已经破坏了本地状态。
   */
  repository_unavailable: { text: 'modsOnline.repositoryRefreshFailed', tone: 'moderate' },
  /**
   * 刷新中：绑定的仓库（或它所在的仓库源）正在重抓 Release，结论还没出来。
   *
   * 与 `unknown`（版本未知）语义不同：unknown 是「查完了也判断不出来」，
   * 刷新中是「还在查」。混用会让刷新过程显示成「版本未知」，
   * 用户会误以为已经出结果了。中性灰，因为此刻没有结论、也不需要动作。
   *
   * 文案是「刷新中」而不是「检测中」：这个状态由**刷新引擎**置位
   * （MOD 安装页的刷新源/刷新全部、MOD 管理页的检查更新都会点亮它），
   * 用户看到的是「后台在刷」，不是「这个 MOD 在自检」。
   */
  refreshing: { text: 'tools.refreshing', tone: 'muted' },
  unknown: { text: 'tools.versionUnknown', tone: 'muted' },
  duplicate: { text: 'modsLocal.duplicate', tone: 'severe' },
  duplicate_lower_version: { text: 'tools.olderVersion', tone: 'mild' },
  load_warning: { text: 'modsLocal.loadWarning', tone: 'critical' },
  disabled: { text: 'modsLocal.disabled', tone: 'muted' },
  /**
   * 开关状态里的「已启用」。
   *
   * 后端 labels 只有 `disabled`；页头把开关状态单独成组（N 已启用 / N 已禁用）
   * 时需要一个对称的 key，所以在汇总层补出来，页头与卡片各自按自己的数据算。
   */
  enabled: { text: 'modsLocal.enabled2', tone: 'accent' },
  /** 包内的纯依赖 DLL：读不出 BepInPlugin 元数据，只作为依赖展示。 */
  dependency: { text: 'tools.dependency', tone: 'muted' },
  /**
   * 依赖未安装：这个 MOD 声明的某个必选依赖没装。
   *
   * 等级取 `critical`（与「加载警告」同档）：缺依赖时 MOD 很可能直接不工作，
   * 比「可更新」更该被看见。但也只是提示 —— 能不能一键装由
   * `modRelations` 判定，没有可安装来源时只显示标签。
   */
  missing_dependency: { text: DEPENDENCY_ISSUE_TEXT.missing, tone: 'critical' },
  dependency_disabled: { text: DEPENDENCY_ISSUE_TEXT.disabled, tone: 'critical' },
  dependency_mismatch: { text: DEPENDENCY_ISSUE_TEXT.version_mismatch, tone: 'critical' },
  /**
   * 兼容关系差一个版本：线上有落在兼容区间里的 Release 可换。
   *
   * 与「缺依赖」分开两档，是因为两者对用户的意义相反：「缺依赖」是「必须
   * 的东西没有」，而这里点一下按钮就能修好。文案与依赖表
   * （`dependencyRows.statusFor` 的 `tools.installable`）用同一套词，卡片与
   * 表格不会各说一种。
   */
  dependency_installable: { text: 'tools.installable', tone: 'moderate' },
  /**
   * 关系冲突：装了不该装的（`conflicts` 命中），或对方版本不在兼容区间内
   * （`compatible` 不满足）。
   *
   * 与「缺依赖」同档 critical，但文案区分：缺依赖是「少东西」，
   * 冲突是「东西不对」，后者通常要卸载而不是安装。
   */
  dependency_conflict: { text: 'modsOnline.conflict', tone: 'critical' },
  /** 仓库文件（RAW）已安装：与「最新版」同档，都用主题色。 */
  installed: { text: 'modsLocal.installed', tone: 'accent' },
  // MOD 安装页专用：仓库维度而不是已安装 MOD 维度
  /** 未安装一律灰色，不区分 Release / RAW。 */
  not_installed: { text: 'overview.notInstalled', tone: 'muted' },
  /** RAW（仓库文件）仓库未安装时也显示「未安装」，不再显示「仓库文件」。 */
  not_installed_raw: { text: 'overview.notInstalled', tone: 'muted' },
  not_refreshed: { text: 'modsOnline.notRefreshed', tone: 'muted' },
  /** 与已安装 MOD 的 `repository_unavailable` 同义，文案与等级保持一致。 */
  refresh_failed: { text: 'modsOnline.repositoryRefreshFailed', tone: 'moderate' },
  repository_disabled: { text: 'modsOnline.disabled', tone: 'muted' },
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
  'refreshing',
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
  const prerelease = isPrereleaseVersion(localVersion(mod), mod.prerelease)
  const tags = labels.filter(label => label !== 'prerelease' || prerelease).map((label: InstalledModStatusLabel | string) => {
    const meta = STATUS_META[label]
    return { key: label, text: meta?.text ?? label, tone: meta?.tone ?? 'muted' }
  })
  if (prerelease && !tags.some((tag) => tag.key === 'prerelease')) {
    const meta = STATUS_META.prerelease
    tags.unshift({ key: 'prerelease', text: meta.text, tone: meta.tone })
  }
  return tags
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
  muted: 4,     // 中性：刷新中、版本未知、未绑定、已禁用
  accent: 5,    // 正常：最新版、已安装
}

/** 同档内的稳定排序（越靠前越需要动作）。 */
const WITHIN_TONE_ORDER: string[] = [
  'load_warning',
  'dependency_conflict',
  'missing_dependency',
  'dependency_disabled',
  'dependency_mismatch',
  'bound_repository_missing',
  'repository_unavailable',
  'dependency_installable',
  'duplicate',
  'update_available',
  'prerelease',
  'refresh_failed',
  'duplicate_lower_version',
  'refreshing',
  'unknown',
  'unbound',
  'disabled',
  'dependency',
  'not_release_asset',
  'installed',
  'latest',
  'enabled',
]

/** 严重度分值：越小越需要用户处理。tone 优先，其次同档固定次序。 */
function statusPriority(key: string): number {
  const meta = STATUS_META[key]
  const tone = meta ? (TONE_PRIORITY[meta.tone] ?? 9) : 9
  const index = WITHIN_TONE_ORDER.indexOf(key)
  return tone * 100 + (index < 0 ? WITHIN_TONE_ORDER.length : index)
}

export interface StatusSummary {
  key: string
  text: string
  tone: StatusTone
  count: number
}

/**
 * 一个「展示单元」对外公开的状态。
 *
 * 页头聚合统计的是页面上的**单元**：单 MOD 一个单元，整个 MOD 包也是一个
 * 单元。包内子 DLL 不作为单元，也不把它们的标签暴露给聚合与定位 ——
 * 包对外只有一个版本结论（见 `packageStatuses`），否则「10 最新版」会被
 * 读成 10 个 DLL，点进去还会高亮包里的某一行。
 */
export interface StatusUnit {
  /** 该单元对外公开的状态 key；重复项会被去重，一个单元只计一次。 */
  keys: readonly string[]
}

/**
 * 按单元汇总状态数量。
 *
 * 排序 = 语义 tone 的严重度（越需要注意越靠前），同档内再按固定业务顺序。
 * 数量为 0 的状态不返回。
 */
export function summarizeUnits(units: readonly StatusUnit[]): StatusSummary[] {
  const counts = new Map<string, number>()
  for (const unit of units) {
    // 同一单元里同一状态只算一次：包里多个 DLL 都命中时仍只代表这一个包
    for (const key of new Set(unit.keys)) {
      counts.set(key, (counts.get(key) ?? 0) + 1)
    }
  }

  return WITHIN_TONE_ORDER
    .filter((key) => (counts.get(key) ?? 0) > 0 && STATUS_META[key])
    .map((key) => ({
      key,
      text: STATUS_META[key].text,
      tone: STATUS_META[key].tone,
      count: counts.get(key) ?? 0,
    }))
    .sort((a, b) => (TONE_PRIORITY[a.tone] ?? 9) - (TONE_PRIORITY[b.tone] ?? 9))
}

/**
 * 包级状态标签。
 *
 * MOD 包是一个整体：同一个 ZIP、同一个仓库、同一个 Release tag，
 * 包内多个 DLL 只是这个包的文件。所以版本结论只该有一条，
 * 不能像页头那样把包内 DLL 的结论数一遍（那会显示成「1 最新版」，
 * 暗示包里还有一个不是最新版的东西）。
 *
 * 因此这里把标签分成四类：
 *   1. 预发布 —— 整包共用一个 tag，不带数量，与单个 MOD 一样放在最前；
 *   2. 依赖告警（缺依赖 / 依赖冲突）—— 仓库 `relations` 挂在仓库上，
 *      包内每个 DLL 都按同一份声明判定，所以这是包级结论：整包只出一条、
 *      不带数量，与单个 MOD 卡片同一顺序同一语义；
 *   3. 版本结论（最新版 / 可更新 / 刷新中 / 版本未知 / 仓库刷新失败…）
 *      —— 整包只取最需要注意的那一条，不带数量；
 *   4. 成员级告警（重复、低版本、加载警告）—— 确实可能只命中某几个 DLL，
 *      保留数量。
 *
 * 纯依赖 DLL 不参与：它没有版本语义，包内行已经单独标了「依赖」。
 */
export interface PackageStatus {
  key: string
  text: string
  tone: StatusTone
  /** 仅成员级告警有数量；版本结论与预发布不显示数量。 */
  count?: number
}

/** 这些标签描述的是包内某个 DLL 的状态，可能只命中一部分成员。 */
const MEMBER_LEVEL_LABELS = new Set(['duplicate', 'duplicate_lower_version', 'load_warning'])

/**
 * 包级依赖告警：按严重度排列（冲突先于缺失）。
 *
 * 依赖声明来自仓库的 `[[repositories.relations]]`，整包共用一份：只要包内
 * 任一成员判定出问题，整个包就不可用，结论属于包而不是某个 DLL。
 * 每类问题只出一条、不带数量；不同类可以同时出现。
 */
const PACKAGE_DEPENDENCY_LABELS = [
  'dependency_conflict',
  'missing_dependency',
  'dependency_disabled',
  'dependency_mismatch',
  'dependency_installable',
] as const

export function packageStatuses(mods: readonly InstalledMod[]): PackageStatus[] {
  const real = mods.filter((mod) => !mod.is_dependency)
  if (!real.length) return []

  const present = new Set<string>()
  for (const mod of real) {
    for (const label of (mod.labels as string[] | undefined) ?? []) {
      if (STATUS_META[label] && (!(mod.bundled_with_dll || mod.release_companion) || !isVersionConclusion(label))) present.add(label)
    }
  }

  const result: PackageStatus[] = []
  /*
   * 预发布与单个 MOD 一致：取「元数据标记」或「版本号带 - 后缀」的或关系，
   * 放在结论前面。整包共用一个 tag，所以只看是否存在，不数个数。
   */
  const prerelease = real.some((mod) => !mod.bundled_with_dll && !mod.release_companion
    && isPrereleaseVersion(localVersion(mod), mod.prerelease))
  if (prerelease) {
    const meta = STATUS_META.prerelease
    result.push({ key: 'prerelease', text: meta.text, tone: meta.tone })
  }

  /*
   * 依赖告警：位置与单个 MOD 一致（预发布之后、版本结论之前）。
   *
   * 这类标签既不是版本结论、也不在成员级告警里，早期实现会**整条丢弃**：
   * 仓库声明了 `requires` 但依赖被卸载时，单 MOD 显示「缺依赖」，
   * 多 DLL 包的标题却什么都不说 —— 同一个包看起来「没问题」。
   */
  for (const key of PACKAGE_DEPENDENCY_LABELS) {
    if (!present.has(key)) continue
    const meta = STATUS_META[key]
    result.push({ key, text: meta.text, tone: meta.tone })
  }

  // 版本结论互斥：整包只保留最需要注意的一条
  const conclusion = [...present]
    .filter(isVersionConclusion)
    .sort((a, b) => statusPriority(a) - statusPriority(b))[0]
  if (conclusion) {
    const meta = STATUS_META[conclusion]
    result.push({ key: conclusion, text: meta.text, tone: meta.tone })
  }

  // 成员级告警：命中几个 DLL 就写几个
  for (const key of WITHIN_TONE_ORDER) {
    if (!MEMBER_LEVEL_LABELS.has(key) || !present.has(key)) continue
    const count = real.filter((mod) => (mod.labels as string[] | undefined)?.includes(key)).length
    if (count <= 0) continue
    const meta = STATUS_META[key]
    result.push({ key, text: meta.text, tone: meta.tone, count })
  }
  return result
}

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
export function repositoryInstallAction(status: StatusTag, repo: ModRepository): InstallAction {
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

/** Only the core plugin can represent a Release; compare it with the current cache. */
export function repositoryStatus(repo: ModRepository, installedMods: InstalledMod[]): StatusTag {
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

  const matches = modsForRepository(installedMods, repo).filter((mod) => mod.repository_binding
    ? matchesReleaseBinding(repo, mod.repository_binding) : isRaw || mod.status !== 'not_release_asset')
  if (isRaw && matches.length) return tag('installed')
  if (!matches.length) return tag(isRaw ? 'not_installed_raw' : repo.cached_releases[0] ? 'not_installed' : 'not_refreshed')
  const installed = matches.reduce<InstalledMod | undefined>((best, mod) => {
    if (mod.is_dependency || mod.release_companion || mod.duplicate_cleanup_available
      || mod.status === 'duplicate_lower_version') return best
    if (!best) return mod
    const currentVersion = localVersion(mod)
    const bestVersion = localVersion(best)
    return currentVersion && (!bestVersion || compareVersions(currentVersion, bestVersion) === 1) ? mod : best
  }, undefined)
  const verdict = installed && releaseUpdateVerdict(installed, repo)
  return tag(verdict === undefined ? 'unknown' : verdict ? 'update_available' : 'latest')
}

function matchesReleaseBinding(repo: ModRepository, binding: NonNullable<InstalledMod['repository_binding']>): boolean {
  return repo.download_mode === 'release' && binding.provider === repo.provider
    && binding.owner.toLowerCase() === repo.username.toLowerCase()
    && binding.repository.toLowerCase() === repo.repository.toLowerCase()
    && (repo.channel === binding.channel || repo.channel === 'all')
}

/** The installation card, local card and refresh summary read the latest
 * cache with the same disk-scan version, even while Rust is rebinding it. */
function releaseUpdateVerdict(mod: InstalledMod, repo: ModRepository): boolean | undefined {
  const local = localVersion(mod)
  if (!local) return undefined
  const binding = mod.repository_binding
  if (binding && !matchesReleaseBinding(repo, binding)) return undefined
  const release = repo.cached_releases.find((release) => {
    if (release.draft) return false
    const channel = binding?.channel ?? repo.channel
    if (channel === 'all') return true
    const beta = isPrereleaseVersion(release.tag_name, release.apiPrerelease, release.prerelease)
    return channel === 'beta' ? beta : !beta
  })
  if (!release) return undefined
  const remote = binding ? release.tag_name : release.version || release.tag_name
  if (binding && remote === local) return false
  const compared = compareVersions(remote, local)
  // Opaque installed Release tags retain exact-tag semantics. An opaque DLL
  // reported after a replacement cannot prove anything about the Release.
  return compared !== null ? compared > 0 : binding?.tag === local ? true : undefined
}

export type ModUpdateVerdict =
  /** 能按当前缓存判定。 */
  | { known: true; updateAvailable: boolean; source: 'release' }
  /** 已有可保留的较新本地副本；不为当前文件重新判定线上版本。 */
  | { known: true; updateAvailable: false; source: 'newer_copy' }
  /** 无法判定（未绑定 / 仓库报错 / RAW / 版本不可比较），调用方保留后端快照。 */
  | { known: false }

export function modUpdateVerdict(mod: InstalledMod, repositories: readonly ModRepository[]): ModUpdateVerdict {
  // 纯依赖不是 MOD，不参与更新判定
  if (mod.is_dependency || mod.release_companion) return { known: false }
  if (mod.duplicate_cleanup_available) return { known: true, updateAvailable: false, source: 'newer_copy' }
  const repo = repositoryForMod(repositories, mod)
  if (!repo || repo.last_error) return { known: false }
  // 仓库文件模式没有版本比较的意义
  if (repo.download_mode === 'repository_file') return { known: false }
  const updateAvailable = releaseUpdateVerdict(mod, repo)
  return updateAvailable === undefined ? { known: false } : { known: true, updateAvailable, source: 'release' }
}

/**
 * 一个已安装 MOD 现在是否可更新（合并「缓存重算」与「后端快照」）。
 *
 * 这是给只需要最终结论的调用方（总览页统计）用的便捷形态；
 * 需要区分「已判定 / 无法判定」的地方（MOD 管理页要显示「检查中」）
 * 直接调 `modUpdateVerdict`。
 */
export function modUpdateAvailable(mod: InstalledMod, repositories: readonly ModRepository[]): boolean {
  const verdict = modUpdateVerdict(mod, repositories)
  return verdict.known ? verdict.updateAvailable : mod.update_available
}
