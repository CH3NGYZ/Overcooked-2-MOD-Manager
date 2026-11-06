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
import { parseVersionSpec, versionMatches } from './versionSpec.ts'
import type { TagTone } from './theme'
import type {
  InstalledMod, MapInfo, RelationKind, ReleaseInfo, ReleaseRelationOverride, Repository,
  RepositoryRelation,
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
 * 区间表达式里能当「下限」用的最高版本（`>=` / `>` / `==`），没有则空串。
 *
 * 只用于「多条覆盖规则都命中时选最具体的一条」：下限越高，说明规则越针对
 * 当前版本。解析失败按「没有下限」处理，与配置校验拒绝非法表达式的口径
 * 一致 —— 判定层不该替配置层报错。
 */
function versionSpecLowerBound(spec: string | null | undefined): string {
  const clauses = parseVersionSpec(spec ?? '')
  if (!clauses) return ''
  let best = ''
  for (const clause of clauses) {
    if (clause.op !== '>=' && clause.op !== '>' && clause.op !== '==') continue
    if (!best || (compareVersions(clause.version, best) ?? 0) > 0) best = clause.version
  }
  return best
}

/**
 * 两条同类关系里更严格的一条（包级依赖表合并用）。
 *
 * 多 DLL 包里每个成员按自己的版本筛 `release_relations`，同一个 GUID 可能拿到
 * 不同要求：低版本成员只有仓库级声明（不设区间），高版本成员命中覆盖规则
 * （`>=0.10.0`）。包对外只需要一个结论，取更严的那条 —— 「至少满足到哪」才有
 * 意义，取松的会把真实的缺依赖藏起来。
 *
 * 只对 `requires` 有严格性概念：`conflicts` / `compatible` 的区间表达的是
 * 「允许的范围」，两个声明并存时该并集而不是取一，所以原样保留先出现的。
 */
export function stricterRelation(
  left: RepositoryRelation,
  right: RepositoryRelation,
): RepositoryRelation {
  if ((left.kind ?? 'requires') !== 'requires' || (right.kind ?? 'requires') !== 'requires') {
    return left
  }
  const rightBound = versionSpecLowerBound(right.version)
  if (!rightBound) return left
  const leftBound = versionSpecLowerBound(left.version)
  if (!leftBound) return right
  return (compareVersions(rightBound, leftBound) ?? 0) > 0 ? right : left
}

/**
 * 版本是否满足一条区间表达式。
 *
 * 空表达式（`""` / `"*"`）表示不设版本要求：只要目标存在就算满足，
 * 哪怕本地读不出版本号 —— 这与「写了要求但读不出版本」必须判不满足
 * 是两件事（后者无法证明满足）。
 */
function matchesVersionSpec(
  spec: string | null | undefined,
  version: string | null | undefined,
): boolean {
  const text = spec?.trim() ?? ''
  if (!text || text === '*') return true
  return versionMatches(text, version ?? '')
}

/**
 * 按版本区间挑出适用的关系表（MOD 仓库与地图条目共用）。
 *
 * 默认值是仓库/地图级 `relations`；`release_relations` 里所有命中的条目中
 * 取**下限最高**的那条，用它的 `relations` **整体替换**默认值（不是合并 ——
 * 合并会让「新版本降低要求」无法表达）。下限相同取先出现的那条：
 * 同一段版本写了两条规则时，顺序就是作者表达的优先级。
 *
 * 传空 `releaseVersion` 时只用默认值：调用方拿不到版本号时，
 * 无法判断该用哪条覆盖规则，硬套一条会比不给更糟。
 */
export function pickReleaseRelations(
  base: readonly RepositoryRelation[],
  overrides: readonly ReleaseRelationOverride[] | undefined,
  releaseVersion: string | null | undefined,
): RepositoryRelation[] {
  const current = releaseVersion?.trim() ?? ''
  if (!current || !overrides?.length) return [...base]

  let best: ReleaseRelationOverride | undefined
  let bestBound = ''
  for (const override of overrides) {
    const spec = override.version?.trim() ?? ''
    if (!spec || !versionMatches(spec, current)) continue
    const bound = versionSpecLowerBound(spec)
    if (!best) {
      best = override
      bestBound = bound
      continue
    }
    // 有下限的规则压过没有下限的；都有下限时取更高的。相等保留先出现的。
    if (bound && (!bestBound || (compareVersions(bound, bestBound) ?? 0) > 0)) {
      best = override
      bestBound = bound
    }
  }
  return best ? [...(best.relations ?? [])] : [...base]
}

/**
 * 取某个 Release 版本适用的关系表。
 *
 * `releaseVersion` 是**这次要装（或当前展示）的版本**，不是本地已装版本：
 * 覆盖规则按它筛，因为不同大版本可能声明不同关系（1.0 依赖 B≥2，1.1 依赖 B≥3）。
 */
export function relationsForRelease(
  repo: Repository,
  releaseVersion: string | null | undefined,
): RepositoryRelation[] {
  return pickReleaseRelations(repo.relations ?? [], repo.release_relations, releaseVersion)
}

/**
 * 取地图条目在某个 Release 版本下的关系表。
 *
 * 地图与 MOD 共用同一套关系模型与同一份覆盖规则，差别只在默认声明的来源：
 * `relations` 是原始声明，`required_frameworks` 是后端给框架提示用的
 * 「依赖」投影（老地图源只有后者），两者都没写时为空。
 */
export function relationsForMapRelease(
  map: Pick<MapInfo, 'relations' | 'required_frameworks' | 'release_relations'>,
  releaseVersion?: string | null,
): RepositoryRelation[] {
  const base = map.relations?.length
    ? map.relations
    : (map.required_frameworks ?? []).map((framework) => ({
      kind: 'requires' as const,
      guid: framework.guid,
      version: framework.version ?? '',
    }))
  return pickReleaseRelations(base, map.release_relations, releaseVersion)
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
 * 这条关系是否适用于该本地 MOD（版本要求 + 预发布三态）。
 *
 * `prerelease` 不写表示两种都算；写了就只在「是不是预发布」相符时才成立。
 * 它先于版本要求判断：区间是「哪些版本」，这个标志是「哪条发布线」。
 */
function relationMatchesCopy(relation: RepositoryRelation, mod: InstalledMod): boolean {
  const wantPrerelease = relation.prerelease
  if (wantPrerelease !== undefined && isPrereleaseCopy(mod) !== wantPrerelease) return false
  return matchesVersionSpec(relation.version, mod.version)
}

/**
 * 在来源仓库里挑一个**满足版本要求**的 Release。
 *
 * 为什么不能只看仓库：要求 B ≥2.0 而该仓库最高版本只有 1.9 时，
 * 装了也解决不了依赖 —— 那种情况必须归为「没有可安装来源」，
 * 而不是给一个点了也没用的按钮。
 *
 * `cached_releases` 已按后端顺序（新的在前）排列，所以从前往后找第一个
 * 满足要求的即可。默认跳过草稿与预发布，避免自动装到测试版；
 * 只有当关系显式要求 `prerelease = true` 时才接受预发布 Release。
 */
function matchingRelease(
  repo: Repository,
  relation: RepositoryRelation,
): ReleaseInfo | undefined {
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
    return matchesVersionSpec(relation.version, version)
  })
}

/**
 * 找到能提供该 GUID、且**有满足区间版本**的在线仓库。
 *
 * 要求仓库本身可用（地区/启用已由调用方筛过）、是 Release 模式、
 * 且缓存里有满足区间的 Release。RAW 仓库不能用来满足关系 ——
 * 它没有版本概念，也没法比较版本要求。
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
 * 需要的是**不满足**这条关系版本要求的版本 —— 通常比冲突区间更新
 * （修复了冲突）或更旧（还没引入冲突）。只在「纯版本冲突」时使用；
 * 发布线互斥（写了 `prerelease`）不走这条路，原因见 `resolveRelation`。
 */
function compatibleSourceFor(
  relation: RepositoryRelation,
  repositories: readonly Repository[],
  excludeRepositoryId?: string,
): { repository: Repository; release: ReleaseInfo } | undefined {
  for (const repo of repositories) {
    if (repo.id === excludeRepositoryId) continue
    if (!repo.plugin_guid || !sameGuid(repo.plugin_guid, relation.guid)) continue
    if (repo.download_mode !== 'release') continue
    const release = (repo.cached_releases ?? []).find((entry) => {
      if (entry.draft || entry.prerelease) return false
      const version = entry.tag_name || entry.version
      // 只要不满足版本要求就可用
      return !matchesVersionSpec(relation.version, version)
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

/* ===== 展示：关系目标是谁 ===== */

/**
 * 一个仓库是否会发布某条发布线的 Release。
 *
 * `true` = 会，`false` = 不会，`undefined` = Release 还没拉到、频道也没限定，
 * 暂时无法判断。
 *
 * 为什么问「会不会发布」而不是「当前属于哪条线」：同一个仓库可以同时发正式版
 * 与测试版（`channel = all`），只看最新一条 Release 会把整仓判成其中一条，
 * 另一条线的精确匹配就永远落空。
 */
function publishesReleaseLine(repo: Repository, line: boolean): boolean | undefined {
  const releases = (repo.cached_releases ?? []).filter((entry) => !entry.draft)
  if (releases.length) {
    return releases.some((entry) =>
      isPrereleaseVersion(entry.tag_name || entry.version, entry.prerelease) === line)
  }
  if (repo.channel === 'beta') return line
  if (repo.channel === 'stable') return !line
  return undefined
}

/**
 * 关系目标在仓库列表里对应的那个 MOD（只用于展示名字）。
 *
 * 关系声明只记 GUID，界面直接摊一串 `com.ch3ngyz.plugin.HostUtilities` 用户
 * 认不出是谁；而**同一个 GUID 可以有多条发布线**（正式版、测试版各一个仓库），
 * 只按 GUID 匹配会挑到错的兄弟仓库。`prerelease` 三态正好把发布线也定下来：
 * GUID + 发布线才能定位到唯一仓库，拿到作者写好的名字。
 *
 * `excludeRepositoryId` 传声明方自己：自冲突指向的是**另一条线**的仓库。
 */
export function relationTargetRepository(
  relation: RepositoryRelation,
  repositories: readonly Repository[],
  excludeRepositoryId?: string,
): Repository | undefined {
  const candidates = repositories.filter((repo) =>
    repo.id !== excludeRepositoryId
    && !!repo.plugin_guid
    && sameGuid(repo.plugin_guid, relation.guid))
  if (!candidates.length) return undefined
  const want = relation.prerelease
  if (want === undefined) return candidates[0]
  const exact = candidates.find((repo) => publishesReleaseLine(repo, want) === true)
  if (exact) return exact
  /*
   * Release 还没拉到时发布线未知（频道 `all`、缓存为空）。只剩一个未知候选时
   * 它就是唯一还能解释这条关系的仓库（声明方已排除，另一条线就剩它）；多个未知
   * 候选则无法判断是哪条线，宁可退回本地 MOD 名，也不猜一个错的兄弟仓库。
   */
  const unknowns = candidates.filter((repo) => publishesReleaseLine(repo, want) === undefined)
  return unknowns.length === 1 ? unknowns[0] : undefined
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

/**
 * 关系标签点下去该跳到哪个仓库（`undefined` 表示没有可跳转的目标）。
 *
 * 「缺依赖」「冲突」描述的是**另一个**插件，所以目标必须是关系解析出的
 * 那个仓库。跳到当前 MOD 自己的仓库是错的：用户在那里看到的是一张
 * 毫无问题的卡片，找不到该处理的东西 —— 这正是这条函数要修的 bug。
 *
 * 没有可安装来源时返回 `undefined`：线上根本没有那个 GUID，跳过去也
 * 无处可去，标签就该保持不可点。
 */
export function relationJumpRepositoryId(
  status: Pick<RelationStatus, 'relation' | 'repository'> | undefined,
): string | undefined {
  return status?.repository?.id
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

/**
 * 卡头那个关系标签该显示什么（`undefined` 表示不显示）。
 *
 * ## 判据是「这张卡装没装」，不是关系种类
 *
 * 关系提醒描述的是**装上之后**会不会因为别的插件而跑不起来，所以它只在卡片
 * 自己已经安装时才有意义（用户 2026-11-04 定的口径：未安装的卡片上不要出现
 * 「缺依赖」）。卡片没装时那一行唯一该说的是「能不能装」；依赖是否满足交给
 * 安装前检查与依赖表 —— 那里本来就是装之前能看到的完整清单。
 *
 * 早期实现让「缺依赖」在未安装卡片上也显示，理由是「装之前就该知道」。实际
 * 效果是：未安装的地图卡片挂着红色「缺依赖」，用户读成「这张图有问题」，而
 * 主按钮明明是「安装」。冲突同理 —— 未安装却标「冲突」，用户不知道该卸载
 * 什么，只有「安装」按钮可点。
 *
 * 已安装之后再报才有意义：用户手动卸掉了前置、或换了不兼容的版本 —— 这时
 * 卡片上的「缺依赖 / 冲突」与「已安装」自洽，也直接指向要修的东西。
 */
export function relationCardTag(
  status: RelationStatus | undefined,
  installed: boolean,
): { kind: 'conflict' | 'missingDependency' | 'installable' } | undefined {
  if (!installed) return undefined
  const view = relationTagView(status)
  return view ? { kind: view.kind } : undefined
}

/* ===== 卡片上的关系提醒标签（四个页面共用一条规则） ===== */

/** 卡片「关系提醒」标签的展示口径（`text` 是 i18n key，视图层 `t()`）。 */
export interface DependencyTagView {
  /** 标签文案的 i18n key。 */
  text: string
  /**
   * 色调：冲突与「缺依赖」取 critical；兼容关系差一个版本（可安装）取
   * moderate —— 有办法解决的事不该比没救的事看起来更严重。
   */
  tone: Extract<TagTone, 'critical' | 'moderate'>
  /** 判定来源：冲突 / 必须依赖缺了 / 兼容版本不在区间（可安装）。 */
  kind: 'conflict' | 'missingDependency' | 'installable'
  /**
   * 点标签该跳到的仓库卡片 id。
   *
   * 只有「有落点」时才给：缺依赖有候选仓库、兼容关系有可换的版本都带；
   * 冲突与无来源的缺依赖跳过去也无处可去，标签保持纯文字。
   */
  repositoryId?: string
}

/**
 * 一条未满足的关系该显示成哪枚提醒。
 *
 * 卡片标签（`dependencyTagFor`）、MOD 卡片状态标签
 * （`modRuntimeStatus.relationStatusLabel`）、地图卡片（`levelSetStatus`）
 * 全部走这一处 —— 三处各写一套时，同一个依赖在不同页面会得到不同结论。
 *
 * 三种关系说三件不同的事（用户 2026-11-04 定的口径）：
 *   · `requires`（必须依赖）：缺了就是缺了，**线上能不能一键补上不改变这件
 *     事** —— 一律写「缺依赖」。写成「可安装」时卡片读起来像「有东西想装」，
 *     而不是「这个 MOD 现在跑不起来」，必须依赖的强制性消失了。
 *   · `compatible`（兼容）：要求「对方存在时版本落在区间内」，线上有满足区间
 *     的版本就是「可安装」，换到那个版本就兼容。
 *   · `conflicts`（冲突）：目标是「你不该装它」，换版本也清不干净旧文件，
 *     唯一出路是卸载 —— 写「有冲突」。
 *
 * `repositoryId` 只在点标签能落到那张仓库卡时给：`requires` 有候选仓库也带
 * 上（跳过去就能补装），只有线上真的没有来源才不带（标签不可点）。
 */
export function relationTagView(status: RelationStatus | undefined): DependencyTagView | undefined {
  if (!status || status.state === 'ok') return undefined
  const kind = status.relation.kind ?? 'requires'
  /*
   * 先按 `kind` 分派，再看 `state`：`compatible` 不满足时 `resolveRelation`
   * 返回的也是 `state: 'conflict'`（「必须处理」这件事相同），先判 state
   * 会把兼容关系读成冲突，把「换个版本就能兼容」错说成「只能卸载」。
   */
  if (kind === 'compatible') {
    if (status.repository) {
      return {
        text: 'tools.installable',
        tone: 'moderate',
        kind: 'installable',
        repositoryId: status.repository.id,
      }
    }
    return { text: 'tools.incompatible', tone: 'critical', kind: 'missingDependency' }
  }
  if (kind === 'conflicts' || status.state === 'conflict') {
    return { text: 'modsOnline.conflict', tone: 'critical', kind: 'conflict' }
  }
  const view: DependencyTagView = {
    text: 'modsOnline.missingDep',
    tone: 'critical',
    kind: 'missingDependency',
  }
  // 只在点得动时才带目标：留一个 `repositoryId: undefined` 的键会让「有没有
  // 落点」有「键不存在」和「键存在但为空」两种表示，消费者与测试都得猜。
  if (status.repository) view.repositoryId = status.repository.id
  return view
}

/**
 * 卡片上那条「关系提醒」标签（MOD 安装 / MOD 管理 / 地图下载 / 地图管理
 * 四页共用；`undefined` = 不显示）。
 *
 * 只回答两件事：卡片自己装没装（没装一律不显示 —— 关系提醒描述的是「装上
 * 之后会不会因为别的插件跑不起来」，挂在未安装的卡片上会被读成「这张卡/这张
 * 图本身有问题」，而主按钮明明是「安装」），以及未满足的关系该说哪句话
 * （`relationTagView`）。
 */
export function dependencyTagFor(
  statuses: readonly RelationStatus[],
  installed: boolean,
): DependencyTagView | undefined {
  if (!installed) return undefined
  return relationTagView(primaryRelation(statuses))
}
