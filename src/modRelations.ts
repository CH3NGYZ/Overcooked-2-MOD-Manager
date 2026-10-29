/**
 * MOD 关系判定：依赖 / 排斥 / 兼容共用一条管线。
 *
 * 三种关系本质上是同一个问题 ——「对某个 GUID 的版本区间有什么要求」，
 * 差别只在满足与违反时的含义：
 *
 *   requires    目标**必须存在**且版本落在区间内   → 缺失/版本不符 = 缺依赖
 *   conflicts   目标**若存在**且版本落在区间内     → 命中 = 冲突，需要卸载
 *   compatible  目标**若存在**，版本必须落在区间内 → 出区间 = 不兼容，需要卸载
 *
 * 为什么合成一张表：作者关心的是「我跟谁有关系」，不是「这属于哪一类关系」；
 * 拆成三张表会让同一件事在三处配置，前端也要三套判定。合成后前端只有
 * `resolveRelations` 一个入口，新增关系种类只加一个 kind 分支。
 *
 * 判定结果分五种，前端据此决定「显示什么」：
 *   ok          没有需要处理的问题
 *   installable 依赖未满足，能匹配到装有该 GUID 的在线仓库 → 给「安装依赖」
 *   missing     依赖未满足，且没有可安装来源 → 只显示「缺依赖」标签
 *   conflict    冲突关系命中，或兼容关系不满足 → 显示「冲突」并给卸载入口
 *
 * 为什么不把冲突也做成「可安装」：装一个东西解决不了「你装了不该装的」，
 * 唯一的解法是卸掉。所以冲突只给卸载，不给安装。
 */
// 带 .ts 扩展名：Node 24 直接执行 TS 时靠它解析相对导入；
// 该文件同时被 Vite 打包，两种加载方式都要能用（与 statusTags 的写法一致）。
import { compareVersions, isPrereleaseVersion } from './modStatus.ts'
import type {
  InstalledMod, RelationKind, ReleaseInfo, Repository, RepositoryRelation,
} from './types'

export type RelationState = 'ok' | 'installable' | 'missing' | 'conflict'

export interface RelationStatus {
  relation: RepositoryRelation
  state: RelationState
  /** 本地已装该 GUID 的版本（可能为 null：装了但没有版本元数据）。 */
  installedVersion: string | null
  /** 命中的本地 MOD（冲突时就是需要卸载的那个）。 */
  installedMod?: InstalledMod
  /** 可安装来源：声明了同一 GUID 的仓库（仅 `installable` 有）。 */
  repository?: Repository
  /**
   * 来源仓库里**满足区间**的那个 Release。
   *
   * 只给仓库不给版本是不够的：要求 ≥2.0 而仓库最高只有 1.9 时，
   * 「装它」根本解决不了依赖，必须提前判定为没有来源。
   */
  release?: ReleaseInfo
}

/** GUID 比较忽略大小写与首尾空白。 */
function sameGuid(left: string, right: string): boolean {
  return left.trim().toLowerCase() === right.trim().toLowerCase()
}

/**
 * 版本是否落在 `[min, max]` 区间内（两端都可省略）。
 *
 * 规则：
 *   - 两端都没写 → 只要存在就算满足
 *   - 写了任一端但本地没版本号 → 按不满足处理（无法证明满足）
 *   - 低于 `min_version` 或高于 `max_version` → 不满足
 *   - 任一方无法解析 → 按不满足处理（宁可提示，也不要静默放过）
 */
export function versionInRange(
  installed: string | null | undefined,
  minVersion?: string,
  maxVersion?: string,
): boolean {
  const min = minVersion?.trim()
  const max = maxVersion?.trim()
  if (!min && !max) return true
  if (!installed?.trim()) return false
  if (min) {
    const compared = compareVersions(installed, min)
    if (compared === null || compared < 0) return false
  }
  if (max) {
    const compared = compareVersions(installed, max)
    // 高于上限同样不满足：依赖库可能在更高版本改了接口
    if (compared === null || compared > 0) return false
  }
  return true
}

/**
 * 取某个 Release 版本适用的关系表。
 *
 * 仓库级 `relations` 是默认值；`release_relations` 里的条目按**版本区间**
 * （`min_version` ≤ 当前 Release 版本 ≤ `max_version`，两端都可省略）
 * 筛选，取「下限最高」的那条命中区间，用它的 `relations` **整体替换**
 * 默认值（不是合并 —— 合并会让「新版本降低要求」无法表达）。
 *
 * 传空 `releaseVersion` 时只用默认值：调用方拿不到版本号时，
 * 无法判断该用哪条覆盖规则，硬套一条会比不给更糟。
 */
export function relationsForRelease(
  repo: Repository,
  releaseVersion: string | null | undefined,
): RepositoryRelation[] {
  const base = repo.relations ?? []
  const overrides = repo.release_relations ?? []
  const current = releaseVersion?.trim()
  if (!current || !overrides.length) return base

  let best: { relations: RepositoryRelation[]; lowerBound: string } | undefined
  for (const override of overrides) {
    const min = override.min_version?.trim() ?? ''
    const max = override.max_version?.trim() ?? ''
    // 下限：当前版本 >= min（未填下限视为不限）
    if (min) {
      const compared = compareVersions(current, min)
      if (compared === null || compared < 0) continue
    }
    // 上限：当前版本 <= max（未填上限视为不限）
    if (max) {
      const compared = compareVersions(current, max)
      if (compared === null || compared > 0) continue
    }
    /*
     * 多条都命中后取下限最高的（最贴近当前版本的那条）。
     *
     * 下限为空视为「最低优先级」：它能匹配一切，不该压过更具体的规则
     * （例如同时写了「≥1.0 全部」和「≥1.8 特别是这个」时，1.9 应命中后者）。
     */
    const isMoreSpecific = !best
      || (best.lowerBound === '' && min !== '')
      || (best.lowerBound !== '' && min !== ''
        && (compareVersions(min, best.lowerBound) ?? 0) > 0)
    if (isMoreSpecific) best = { relations: override.relations ?? [], lowerBound: min }
  }
  return best ? best.relations : base
}

/** 本地所有装了某个 GUID 的 MOD（含被禁用的 —— 禁用不等于没装）。 */
function installedForGuid(mods: readonly InstalledMod[], guid: string): InstalledMod[] {
  return mods.filter((mod) => !!mod.guid && sameGuid(mod.guid, guid))
}

/**
 * 一个本地 MOD 是否属于「预发布」。
 *
 * 两个来源取或：安装清单的 `prerelease` 标记（权威），以及版本号本身
 * 是不是四段/带后缀写法（旧快照兜底）。与 `statusTags` 的展示判定
 * 用同一套规则，避免「标签说预发布、关系判定说正式版」。
 */
function isPrereleaseCopy(mod: InstalledMod): boolean {
  return isPrereleaseVersion(mod.version, mod.prerelease)
}

/**
 * 这条关系是否适用于该本地 MOD（版本区间 + 预发布三态）。
 *
 * `prerelease` 不写表示两种都算；写了就只在「是不是预发布」相符时才成立。
 * 它先于版本区间判断：区间是「哪些版本」，这个标志是「哪条发布线」。
 */
function relationMatchesCopy(relation: RepositoryRelation, mod: InstalledMod): boolean {
  const wantPrerelease = relation.prerelease
  if (wantPrerelease !== undefined && isPrereleaseCopy(mod) !== wantPrerelease) return false
  return versionInRange(mod.version, relation.min_version, relation.max_version)
}

/**
 * 在来源仓库里挑一个**满足区间**的 Release。
 *
 * 为什么不能只看仓库：要求 B ≥2.0 而该仓库最高版本只有 1.9 时，
 * 装了也解决不了依赖 —— 那种情况必须归为「没有可安装来源」，
 * 而不是给一个点了也没用的按钮。
 *
 * `cached_releases` 已按后端顺序（新的在前）排列，所以从前往后找第一个
 * 满足区间的即可。默认跳过草稿与预发布，避免自动装到测试版；
 * 只有当关系显式要求 `prerelease = true` 时才接受预发布 Release。
 */
function matchingRelease(
  repo: Repository,
  relation: RepositoryRelation,
): ReleaseInfo | undefined {
  const min = relation.min_version?.trim()
  const max = relation.max_version?.trim()
  const wantPrerelease = relation.prerelease
  return (repo.cached_releases ?? []).find((release) => {
    if (release.draft) return false
    const version = release.tag_name || release.version
    // 发布线判定：默认只要正式版；显式要求预发布时才放宽
    const prerelease = isPrereleaseVersion(version, release.prerelease)
    if (wantPrerelease === true) {
      if (!prerelease) return false
    } else if (prerelease) {
      return false
    }
    return versionInRange(version, min, max)
  })
}

/**
 * 找到能提供该 GUID、且**有满足区间版本**的在线仓库。
 *
 * 要求仓库本身可用（地区/启用已由调用方筛过）、是 Release 模式、
 * 且缓存里有满足区间的 Release。RAW 仓库不能用来满足关系 ——
 * 它没有版本概念，也没法比较 `min_version`。
 *
 * `excludeRepositoryId` 排除仓库自身：一个 MOD 声明关于自己的关系时，
 * 「装自己」既解决不了依赖也消不掉冲突，只会产生一个点不动按钮。
 */
function installSourceFor(
  guid: string,
  repositories: readonly Repository[],
  relation: RepositoryRelation,
  excludeRepositoryId?: string,
): { repository: Repository; release: ReleaseInfo } | undefined {
  for (const repo of repositories) {
    if (repo.id === excludeRepositoryId) continue
    if (!repo.plugin_guid || !sameGuid(repo.plugin_guid, guid)) continue
    if (repo.download_mode !== 'release') continue
    const release = matchingRelease(repo, relation)
    if (release) return { repository: repo, release }
  }
  return undefined
}

/**
 * 解析一条关系声明的状态。
 *
 * 三种 kind 的判定差异集中在这里，其它地方不需要再判断 kind：
 *   requires    装了且版本在区间内 → ok；否则找安装来源（installable/missing）
 *   conflicts   装了且命中区间 → conflict；否则 ok（没装就没有冲突）
 *   compatible  没装 → ok（不兼容只在对方存在时才成立）；
 *               装了但版本在区间外 → conflict
 *
 * `owner` 是**声明这条关系的仓库**，只用于排除「装自己」这种无意义来源。
 *
 * 自冲突（正式版声明「与同 GUID 的测试版互斥」）靠 `prerelease` 三态自然
 * 消解：owner 自己的副本必定在标志的另一侧，不会命中。因此 `conflicts` /
 * `compatible` 指向自己的 `plugin_guid` 时必须写这个标志 —— 导入校验会拒绝
 * 没有标志的自冲突（那是「我不能和自己共存」，永远为真）。
 */
export function resolveRelation(
  relation: RepositoryRelation,
  mods: readonly InstalledMod[],
  repositories: readonly Repository[],
  owner?: Repository,
): RelationStatus {
  const kind: RelationKind = relation.kind ?? 'requires'
  // 依赖看全部副本（存在即可）；冲突/兼容同样看全部，靠 prerelease 区分发布线
  const installed = installedForGuid(mods, relation.guid)
  const target = installed[0]
  const installedVersion = target?.version ?? null

  if (kind === 'requires') {
    // 装了且版本落在区间内才算满足；装了多份时任一份满足即可
    if (installed.some((mod) => relationMatchesCopy(relation, mod))) {
      return { relation, state: 'ok', installedVersion, installedMod: target }
    }
    /*
     * 已装但版本不符：仍然要给出「可安装」入口 —— 用户需要的是升级/降级到
     * 依赖要求的那个版本，而不是再装一遍。
     */
    const source = installSourceFor(relation.guid, repositories, relation, owner?.id)
    return {
      relation,
      state: source ? 'installable' : 'missing',
      installedVersion,
      installedMod: target,
      repository: source?.repository,
      release: source?.release,
    }
  }

  if (kind === 'conflicts') {
    // 目标存在且命中（版本区间 + 发布线）→ 冲突
    const hit = installed.find((mod) => relationMatchesCopy(relation, mod))
    if (!hit) return { relation, state: 'ok', installedVersion }
    /*
     * 冲突的解法看它是哪一类：
     *   · 纯版本区间冲突（同一发布线内）→ 线上有区间外版本就换版本；
     *   · 发布线互斥（写了 prerelease）→ 换版本等于换发布线，装同一
     *     GUID 的另一个包并不能把旧发布线的文件清干净，所以只能卸载。
     */
    const source = relation.prerelease === undefined
      ? compatibleSourceFor(relation, repositories, owner?.id)
      : undefined
    return {
      relation,
      state: 'conflict',
      installedVersion: hit.version ?? null,
      installedMod: hit,
      repository: source?.repository,
      release: source?.release,
    }
  }

  // kind === 'compatible'：对方存在时版本必须落在区间内
  if (!target) return { relation, state: 'ok', installedVersion: null }
  if (relationMatchesCopy(relation, target)) {
    return { relation, state: 'ok', installedVersion, installedMod: target }
  }
  // 不在兼容区间内：线上有区间内的版本就能靠升级/降级救回来
  const source = installSourceFor(relation.guid, repositories, relation, owner?.id)
  return {
    relation,
    state: 'conflict',
    installedVersion,
    installedMod: target,
    repository: source?.repository,
    release: source?.release,
  }
}

/** 这个本地 MOD 是否来自该仓库（按 id 或结构化绑定判断）。 */
export function belongsToRepository(mod: InstalledMod, repo: Repository): boolean {
  if (mod.repository_id === repo.id) return true
  if (mod.repository_ids?.includes(repo.id)) return true
  const binding = mod.repository_binding
  if (!binding) return false
  return binding.provider === repo.provider
    && binding.owner.toLowerCase() === repo.username.toLowerCase()
    && binding.repository.toLowerCase() === repo.repository.toLowerCase()
}

/**
 * 为冲突关系找一个「装上就不冲突」的版本。
 *
 * 需要的是**不命中**这条关系的版本 —— 通常比冲突区间更新（修复了冲突）
 * 或更旧（还没引入冲突）。只在「纯版本区间冲突」时使用；发布线互斥
 * （写了 `prerelease`）不走这条路，原因见 `resolveRelation`。
 */
function compatibleSourceFor(
  relation: RepositoryRelation,
  repositories: readonly Repository[],
  excludeRepositoryId?: string,
): { repository: Repository; release: ReleaseInfo } | undefined {
  const min = relation.min_version?.trim()
  const max = relation.max_version?.trim()
  for (const repo of repositories) {
    if (repo.id === excludeRepositoryId) continue
    if (!repo.plugin_guid || !sameGuid(repo.plugin_guid, relation.guid)) continue
    if (repo.download_mode !== 'release') continue
    const release = (repo.cached_releases ?? []).find((entry) => {
      if (entry.draft || entry.prerelease) return false
      const version = entry.tag_name || entry.version
      // 只要不命中区间就可用
      return !versionInRange(version, min, max)
    })
    if (release) return { repository: repo, release }
  }
  return undefined
}

/**
 * 一个仓库声明的所有关系的状态。
 *
 * 可选依赖（`optional: true`）不参与判定：它们缺失是正常状态，
 * 提示出来只会变成噪音。
 *
 * `releaseVersion` 用于按 Release 覆盖关系表：同一 MOD 的不同大版本可以
 * 依赖不同版本的基础库（A 1.0 要 B ≥2.0，A 1.1 要 B ≥3.0）。
 * 不传时只用仓库级默认声明。
 */
export function resolveRelations(
  repo: Repository,
  mods: readonly InstalledMod[],
  repositories: readonly Repository[],
  releaseVersion?: string | null,
): RelationStatus[] {
  const declared = relationsForRelease(repo, releaseVersion)
  const relevant = declared.filter((relation) =>
    !(relation.kind === 'requires' && relation.optional))
  if (!relevant.length) return []
  /*
   * 候选来源排除仓库自身：一个 MOD 声明依赖自己的 GUID 是配置错误，
   * 若把自己当作来源，按钮会「安装自己」，永远不解决依赖。
   */
  const candidates = repositories.filter((candidate) => candidate.id !== repo.id)
  return relevant.map((relation) => resolveRelation(relation, mods, candidates, repo))
}

/** 汇总：所有未满足的状态（用于决定卡片显示什么、按钮指向哪条）。 */
export function unmetRelations(statuses: readonly RelationStatus[]): RelationStatus[] {
  return statuses.filter((status) => status.state !== 'ok')
}

/**
 * 挑出最该被卡片展示的那一条。
 *
 * 冲突优先于依赖：缺依赖时 MOD 只是不工作，而冲突可能已经让游戏崩了；
 * 同一档里优先「能直接处理」的那条（installable 有按钮）。
 */
export function primaryRelation(
  statuses: readonly RelationStatus[],
): RelationStatus | undefined {
  const unmet = unmetRelations(statuses)
  return unmet.find((status) => status.state === 'conflict')
    ?? unmet.find((status) => status.state === 'installable')
    ?? unmet[0]
}

/* ===== 安装前计划 ===== */

/**
 * 安装一个 MOD 之前要做的全部事情。
 *
 * 三件事合成一个计划，是因为它们的解法互相影响：
 *   1. **卸载冲突**：装了不该装的东西，必须先清掉，否则装上也白装
 *   2. **改版本**：兼容关系不满足、依赖版本不符时，装对的版本比卸载更好
 *   3. **装依赖**：缺的东西最后补，此时其它关系已经理顺
 *
 * 顺序固定为 卸载 → 改版本 → 装依赖：先腾地方，再补版本，最后补缺件。
 * 反过来的话，「装依赖」可能装上一个马上要被卸掉的版本。
 */
export interface InstallPlan {
  /** 必须先卸载的本地 MOD（同 GUID 冲突、区间命中）。 */
  uninstall: RelationStatus[]
  /** 需要装（升级或降级到指定 Release）的条目，按声明顺序。 */
  install: RelationStatus[]
  /** 无法自动处理、只能提醒用户的关系。 */
  blocked: RelationStatus[]
}

/**
 * 生成安装前计划。
 *
 * `releaseVersion` 是**这次要装的版本**：关系表按它筛 `release_relations`，
 * 因为不同大版本可能声明不同关系（1.0 依赖 B≥2，1.1 依赖 B≥3）。
 *
 * 自动可处理的判定：
 *   - 冲突（conflicts 命中）→ 线上有非冲突区间版本时「改版本」，
 *     否则「卸载」；两者都做不到才算 blocked
 *   - 不兼容（compatible 不满足）→ 线上有区间内版本时「改版本」，
 *     否则 blocked（卸载对方也解决不了，问题在版本上）
 *   - 缺依赖 → 线上有满足区间的版本时「装依赖」，否则 blocked
 */
export function buildInstallPlan(
  repo: Repository,
  mods: readonly InstalledMod[],
  repositories: readonly Repository[],
  releaseVersion?: string | null,
): InstallPlan {
  const statuses = resolveRelations(repo, mods, repositories, releaseVersion)
  const plan: InstallPlan = { uninstall: [], install: [], blocked: [] }

  for (const status of statuses) {
    if (status.state === 'ok') continue
    const kind = status.relation.kind ?? 'requires'
    const hasSource = !!status.repository && !!status.release

    if (kind === 'conflicts') {
      // 有「不在冲突区间内的版本」就优先换版本，比卸载更贴合用户意图
      if (hasSource) plan.install.push(status)
      else plan.uninstall.push(status)
      continue
    }

    if (kind === 'compatible') {
      // 不在兼容区间内：线上有区间内版本就能靠升级/降级救回来
      if (hasSource) plan.install.push(status)
      // 卸载对方并不能解决「版本不对」，只能提醒
      else plan.blocked.push(status)
      continue
    }

    // requires：有满足区间的来源就装，没有就只能提醒
    if (status.state === 'installable' && hasSource) plan.install.push(status)
    else plan.blocked.push(status)
  }

  return plan
}

/** 计划里是否有任何需要用户决定的动作。 */
export function planHasActions(plan: InstallPlan): boolean {
  return plan.uninstall.length > 0 || plan.install.length > 0 || plan.blocked.length > 0
}
