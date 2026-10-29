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
  /**
   * 缺依赖：这个 MOD 声明的某个必选依赖没装（或版本不够）。
   *
   * 等级取 `critical`（与「加载警告」同档）：缺依赖时 MOD 很可能直接不工作，
   * 比「可更新」更该被看见。但也只是提示 —— 能不能一键装由
   * `modDependencies` 判定，没有可安装来源时只显示标签。
   */
  missing_dependency: { text: '缺依赖', tone: 'critical' },
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
  'missing_dependency',
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
 * 因此这里把标签分成三类：
 *   1. 版本结论（最新版 / 可更新 / 检测中 / 版本未知 / 仓库刷新失败…）
 *      —— 整包只取最需要注意的那一条，不带数量；
 *   2. 预发布 —— 整包共用一个 tag，也不带数量，与单个 MOD 一样放在最前；
 *   3. 成员级告警（重复、低版本、加载警告）—— 确实可能只命中某几个 DLL，
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

export function packageStatuses(mods: readonly InstalledMod[]): PackageStatus[] {
  const real = mods.filter((mod) => !mod.is_dependency)
  if (!real.length) return []

  const present = new Set<string>()
  for (const mod of real) {
    for (const label of (mod.labels as string[] | undefined) ?? []) {
      if (STATUS_META[label]) present.add(label)
    }
  }

  const result: PackageStatus[] = []
  /*
   * 预发布与单个 MOD 一致：取「元数据标记」或「版本号带 - 后缀」的或关系，
   * 放在结论前面。整包共用一个 tag，所以只看是否存在，不数个数。
   */
  const prerelease = real.some((mod) => isPrereleaseVersion(localVersion(mod), mod.prerelease))
  if (prerelease) {
    const meta = STATUS_META.prerelease
    result.push({ key: 'prerelease', text: meta.text, tone: meta.tone })
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
 * 用当前 Release 缓存判断一个已安装 MOD 的更新状态 —— 唯一的判定入口。
 *
 * 为什么需要重算：MOD 的 `labels` / `update_available` 是后端**扫描时**
 * 算好的，而仓库 Release 是之后异步刷新回来的。刷新期间 Dashboard 不重读
 * （那会重扫磁盘），所以标签会停在旧结论，直到整批结束才更新。
 * 这里按同一套版本规则用当前 Release 缓存重算，让「刷完一个仓库 →
 * 该仓库绑定的 MOD 立刻显示最新版/可更新」，不需要 IPC，也不碰磁盘。
 *
 * **所有需要「这个 MOD 现在能不能更新」的地方都必须调它。** 曾经拆成
 * 「返回可空的重算」+ 一层包装函数，结果总览页用错了一个 —— 刷新完 Release
 * 后管理页显示可更新、总览写着全部最新。合并成一个函数后没有选择空间。
 *
 * `mods.ts` 里的后端扫描用同样规则算 `update_available`，两者必须一致。
 */
export type ModUpdateVerdict =
  /** 能按当前缓存判定。 */
  | { known: true; updateAvailable: boolean }
  /** 无法判定（未绑定 / 仓库报错 / RAW / 版本不可比较），调用方保留后端快照。 */
  | { known: false }

export function modUpdateVerdict(mod: InstalledMod, repositories: Repository[]): ModUpdateVerdict {
  // 纯依赖不是 MOD，不参与更新判定
  if (mod.is_dependency) return { known: false }
  const repo = repositoryForMod(repositories, mod)
  if (!repo || repo.last_error) return { known: false }
  // 仓库文件模式没有版本比较的意义
  if (repo.download_mode === 'repository_file') return { known: false }
  const release = repo.cached_releases[0]
  if (!release) return { known: false }
  const compared = compareVersions(release.version || release.tag_name, localVersion(mod))
  if (compared === null) return { known: false }
  return { known: true, updateAvailable: compared > 0 }
}

/**
 * 一个已安装 MOD 现在是否可更新（合并「缓存重算」与「后端快照」）。
 *
 * 这是给只需要最终结论的调用方（总览页统计）用的便捷形态；
 * 需要区分「已判定 / 无法判定」的地方（MOD 管理页要显示「检查中」）
 * 直接调 `modUpdateVerdict`。
 */
export function modUpdateAvailable(mod: InstalledMod, repositories: Repository[]): boolean {
  const verdict = modUpdateVerdict(mod, repositories)
  return verdict.known ? verdict.updateAvailable : mod.update_available
}
