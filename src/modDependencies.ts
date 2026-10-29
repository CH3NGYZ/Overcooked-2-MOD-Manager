/**
 * MOD 依赖的满足判定。
 *
 * 仓库源可以在 `requires` 里声明「我需要哪些插件先装好」，用 GUID 表达
 * （与本地绑定用的 `plugin_guid` 同一套标识 —— 插件名和文件名都会被作者改写，
 * 只有 GUID 稳定）。
 *
 * 判定结果分三种，前端据此决定「显示什么」：
 *   - satisfied   已装且版本满足 → 什么都不显示
 *   - installable 未满足，但能匹配到装有该 GUID 的在线仓库 → 给「安装依赖」按钮
 *   - missing     未满足，且没有可安装来源 → 只显示「缺依赖」标签，不给按钮
 *
 * 为什么不合并成「缺 / 不缺」：用户看到「安装依赖」按钮时期望点了就能解决；
 * 如果仓库里根本没有这个 GUID，按钮点下去只会失败。分开表示才对得上能力。
 */
// 带 .ts 扩展名：Node 24 直接执行 TS 时靠它解析相对导入；
// 该文件同时被 Vite 打包，两种加载方式都要能用（与 statusTags 的写法一致）。
import { compareVersions } from './modStatus.ts'
import type { InstalledMod, Repository, RepositoryDependency } from './types'

export type DependencyState = 'satisfied' | 'installable' | 'missing'

export interface DependencyStatus {
  dependency: RepositoryDependency
  state: DependencyState
  /** 本地已装版本（可能为 null：装了但没有版本元数据）。 */
  installedVersion: string | null
  /** 可安装来源：声明了同一 GUID 的仓库。 */
  repository?: Repository
}

/** GUID 比较忽略大小写与首尾空白。 */
function sameGuid(left: string, right: string): boolean {
  return left.trim().toLowerCase() === right.trim().toLowerCase()
}

/**
 * 取某个 Release 版本适用的依赖列表。
 *
 * 仓库级 `requires` 是默认值；`release_requires` 里的条目按
 * 「`version` ≤ 当前 Release 版本」筛选，取**匹配到的最高那条**，
 * 用它的 `requires` **整体替换**默认值（不是合并 ——
 * 合并会让「新版本降低要求」无法表达）。
 *
 * 传空 `releaseVersion` 时只用默认值：调用方拿不到版本号时，
 * 无法判断该用哪条覆盖规则，硬套一条会比不给更糟。
 */
export function requiresForRelease(
  repo: Repository,
  releaseVersion: string | null | undefined,
): RepositoryDependency[] {
  const base = repo.requires ?? []
  const overrides = repo.release_requires ?? []
  const current = releaseVersion?.trim()
  if (!current || !overrides.length) return base

  let best: { requires: RepositoryDependency[]; matched: string } | undefined
  for (const override of overrides) {
    const gate = override.version?.trim()
    if (!gate) continue
    const compared = compareVersions(current, gate)
    // 当前版本 >= 该条的生效版本 才算命中
    if (compared === null || compared < 0) continue
    if (!best || (compareVersions(gate, best.matched) ?? -1) > 0) {
      best = { requires: override.requires ?? [], matched: gate }
    }
  }
  return best ? best.requires : base
}

/** 本地是否装了某个 GUID（含被禁用的插件 —— 禁用不等于没装）。 */
function installedForGuid(mods: readonly InstalledMod[], guid: string): InstalledMod | undefined {
  return mods.find((mod) => !!mod.guid && sameGuid(mod.guid, guid))
}

/**
 * 版本是否满足最低要求。
 *
 * 规则：
 *   - 没写 `min_version` → 只要装了就算满足
 *   - 写了但本地没版本号 → 按不满足处理（无法证明够新）
 *   - 任一方无法解析 → 按不满足处理（宁可提示，也不要静默放过）
 */
function versionSatisfies(installed: string | null, minVersion: string | undefined): boolean {
  const required = minVersion?.trim()
  if (!required) return true
  if (!installed?.trim()) return false
  const compared = compareVersions(installed, required)
  return compared !== null && compared >= 0
}

/**
 * 解析一个依赖声明的状态。
 *
 * `repository` 只在该 GUID 确实能装上时才给出：要求仓库本身可用
 * （地区/启用已由调用方筛过）、是 Release 模式、且缓存里有 Release。
 * RAW 仓库不能用来满足依赖 —— 它没有版本概念，也没法比较 `min_version`。
 */
export function resolveDependency(
  dependency: RepositoryDependency,
  mods: readonly InstalledMod[],
  repositories: readonly Repository[],
): DependencyStatus {
  const installed = installedForGuid(mods, dependency.guid)
  const installedVersion = installed?.version ?? null

  if (installed && versionSatisfies(installedVersion, dependency.min_version)) {
    return { dependency, state: 'satisfied', installedVersion }
  }

  /*
   * 已装但版本不够：仍然要给出「可安装」入口 —— 用户需要的是升级到依赖要求的
   * 那个版本，而不是再装一遍。README/预览里能看到具体装的是哪个版本。
   */
  const installable = repositories.find((repo) =>
    !!repo.plugin_guid
    && sameGuid(repo.plugin_guid, dependency.guid)
    && repo.download_mode === 'release'
    && (repo.cached_releases?.length ?? 0) > 0)

  return {
    dependency,
    state: installable ? 'installable' : 'missing',
    installedVersion,
    repository: installable,
  }
}

/**
 * 一个仓库声明的所有**必选**依赖的状态。
 *
 * 可选依赖（`optional: true`）不参与判定：它们缺失是正常状态，
 * 提示出来只会变成噪音。
 *
 * `releaseVersion` 用于按 Release 覆盖依赖：同一 MOD 的不同大版本可以
 * 依赖不同版本的基础库（A 1.0 要 B ≥2.0，A 1.1 要 B ≥3.0）。
 * 不传时只用仓库级默认声明。
 */
export function resolveDependencies(
  repo: Repository,
  mods: readonly InstalledMod[],
  repositories: readonly Repository[],
  releaseVersion?: string | null,
): DependencyStatus[] {
  const declared = requiresForRelease(repo, releaseVersion)
  const required = declared.filter((dependency) => !dependency.optional)
  if (!required.length) return []
  /*
   * 查找可安装来源时排除仓库自身：一个 MOD 声明依赖自己的 GUID 是配置错误，
   * 若把自己当作来源，按钮会「安装自己」，永远不解决依赖。
   */
  const candidates = repositories.filter((candidate) => candidate.id !== repo.id)
  return required.map((dependency) => resolveDependency(dependency, mods, candidates))
}

/** 汇总：只要有一条未满足，就返回它（用于决定卡片是否显示提示）。 */
export function firstUnmetDependency(statuses: readonly DependencyStatus[]): DependencyStatus | undefined {
  return statuses.find((status) => status.state !== 'satisfied')
}
