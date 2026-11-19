/**
 * Rust 关系评估的前端投影，以及来源声明的展示、版本覆盖与名称格式化。
 * 运行状态只读取 modChanges/evaluationProjection 的批量快照；这里不求解安装候选。
 */
// 带 .ts 扩展名：Node 24 直接执行 TS 时靠它解析相对导入；
// 该文件同时被 Vite 打包，两种加载方式都要能用（与 statusTags 的写法一致）。
import { compareVersions, isPrereleaseVersion } from './modStatus.ts'
import { relationForSubject, relationsForSubject, repairRoot, targetMod } from './modChanges/evaluationProjection.ts'
import type { EvaluationSubject, RelationEvaluation, RootIntent } from './modChanges/types'
import { parseVersionSpec, versionMatches } from './versionSpec.ts'
import type { TagTone } from './theme'
import type {
  InstalledMod, MapInfo, ReleaseInfo, ReleaseRelationOverride, ModRepository,
  RepositoryRelation,
} from './types'

export type RelationState = 'ok' | 'installable' | 'missing' | 'conflict' | 'unknown' | 'unassessed' | 'disabled' | 'version_mismatch'

export interface RelationStatus {
  relation: RepositoryRelation
  state: RelationState
  /** 本地已装该 GUID 的版本（可能为 null：装了但没有版本元数据）。 */
  installedVersion: string | null
  /** 命中的本地 MOD（冲突时就是需要卸载的那个）。 */
  installedMod?: InstalledMod
  /** 可安装来源：声明了同一 GUID 的仓库（仅 `installable` 有）。 */
  repository?: ModRepository
  /**
   * 来源仓库里**满足区间**的那个 Release。
   *
   * 只给仓库不给版本是不够的：要求 ≥2.0 而仓库最高只有 1.9 时，
   * 「装它」根本解决不了依赖，必须提前判定为没有来源。
   */
  release?: ReleaseInfo
  repair?: RootIntent
}

/** GUID 比较忽略大小写与首尾空白。 */
export function sameGuid(left: string, right: string): boolean {
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
export interface RelationSubject {
  id: string
  name: string
  name_i18n?: Record<string, string>
  cached_releases: ReleaseInfo[]
  relations?: RepositoryRelation[]
  release_relations?: ReleaseRelationOverride[]
}

export function relationsForRelease(
  repo: RelationSubject,
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
export function installedForGuid(mods: readonly InstalledMod[], guid: string): InstalledMod[] {
  return mods.filter((mod) => !!mod.guid && sameGuid(mod.guid, guid))
}

/** Project an authoritative relation result into the existing display model. */
export function relationStatusFromEvaluation(
  evaluation: RelationEvaluation,
  mods: readonly InstalledMod[],
): RelationStatus {
  const installedMod = targetMod(evaluation, mods)
  const state: RelationState = evaluation.state === 'satisfied' || evaluation.state === 'optional'
    ? 'ok' : evaluation.state
  return { relation: evaluation.relation, state, installedMod, installedVersion: installedMod?.version ?? null,
    ...(state !== 'ok' && state !== 'unknown' ? { repair: repairRoot(evaluation.owner_id) } : {}) }
}

/** A declaration can be displayed before its exact subject has been evaluated. */
function unassessedRelation(relation: RepositoryRelation): RelationStatus {
  return { relation, state: 'unassessed', installedVersion: null }
}

export function resolveRelation(
  relation: RepositoryRelation,
  mods: readonly InstalledMod[],
  _repositories: readonly ModRepository[],
  owner?: Pick<RelationSubject, 'id'>,
  subject?: EvaluationSubject,
): RelationStatus {
  const evaluation = relationForSubject(relation, subject ?? (owner ? { repository_id: owner.id } : undefined))
  return evaluation ? relationStatusFromEvaluation(evaluation, mods)
    : unassessedRelation(relation)
}

/** A known empty assessment still overrides source declarations. */
export function relationStatusesForSubject(
  subject: EvaluationSubject | undefined,
  mods: readonly InstalledMod[],
  declarations?: () => readonly RepositoryRelation[],
): RelationStatus[] {
  const evaluations = subject ? relationsForSubject(subject) : undefined
  return evaluations
    ? evaluations.filter(({ relation }) => !relation.optional).map(entry => relationStatusFromEvaluation(entry, mods))
    : (declarations?.() ?? []).filter(relation => !relation.optional).map(unassessedRelation)
}

/** 这个本地 MOD 是否来自该仓库（按 id 或结构化绑定判断）。 */
export function belongsToRepository(mod: InstalledMod, repo: ModRepository): boolean {
  if (mod.repository_id === repo.id) return true
  if (mod.repository_ids?.includes(repo.id)) return true
  const binding = mod.repository_binding
  if (!binding) return false
  return binding.provider === repo.provider
    && binding.owner.toLowerCase() === repo.username.toLowerCase()
    && binding.repository.toLowerCase() === repo.repository.toLowerCase()
}

/** A subject's rows come from the backend, including empty overrides. */
export function resolveRelations(
  repo: RelationSubject,
  mods: readonly InstalledMod[],
  _repositories: readonly ModRepository[],
  releaseVersion?: string | null,
): RelationStatus[] {
  const subject = { repository_id: repo.id, ...(releaseVersion ? { tag: releaseVersion } : {}) }
  return relationStatusesForSubject(subject, mods, () => relationsForRelease(repo, releaseVersion))
}

function relationIsUnmet(status: RelationStatus): boolean {
  return status.state !== 'ok' && status.state !== 'unknown' && status.state !== 'unassessed'
}

/** 只汇总已确认的问题，供卡头、悬停说明和处理按钮使用。 */
export function unmetRelations(statuses: readonly RelationStatus[]): RelationStatus[] {
  return statuses.filter(relationIsUnmet)
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
function publishesReleaseLine(repo: ModRepository, line: boolean): boolean | undefined {
  const releases = (repo.cached_releases ?? []).filter((entry) => !entry.draft)
  if (releases.length) {
    return releases.some((entry) =>
      isPrereleaseVersion(entry.tag_name || entry.version, entry.apiPrerelease, entry.prerelease) === line)
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
  repositories: readonly ModRepository[],
  excludeRepositoryId?: string,
): ModRepository | undefined {
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
  status: Pick<RelationStatus, 'relation' | 'repository' | 'repair'> | undefined,
): string | undefined {
  return status?.repair?.node_id ?? status?.repository?.id
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
  if (!status || !relationIsUnmet(status)) return undefined
  if (status.state === 'disabled') return { text: 'modChanges.relation.disabled', tone: 'critical', kind: 'missingDependency' }
  if (status.state === 'version_mismatch') return { text: 'modChanges.relation.versionMismatch', tone: 'critical', kind: 'missingDependency' }
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
