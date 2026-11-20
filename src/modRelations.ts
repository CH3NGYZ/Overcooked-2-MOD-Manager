/**
 * Rust 关系评估的前端投影，以及来源声明的展示、版本覆盖与名称格式化。
 * 运行状态只读取 modChanges/evaluationProjection 的批量快照；这里不求解安装候选。
 */
// 带 .ts 扩展名：Node 24 直接执行 TS 时靠它解析相对导入；
// 该文件同时被 Vite 打包，两种加载方式都要能用（与 statusTags 的写法一致）。
import { compareVersions, isPrereleaseVersion } from './modStatus.ts'
import { relationForSubject, relationsForSubject, repairTarget, targetMod } from './modChanges/evaluationProjection.ts'
import type { DependencyRepairTarget, EvaluationSubject, RelationEvaluation } from './modChanges/types'
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
  /** 评估提供的具体原因；展示层不重新推导判断。 */
  reason?: string
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
  repair?: DependencyRepairTarget
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
  return { relation: evaluation.relation, state, reason: evaluation.reason, installedMod, installedVersion: installedMod?.version ?? null,
    ...(state !== 'ok' && state !== 'unknown' ? { repair: repairTarget(evaluation.owner_id) } : {}) }
}

/** A declaration can be displayed before its exact subject has been evaluated. */
function unassessedRelation(relation: RepositoryRelation): RelationStatus {
  return { relation, state: 'unassessed', installedVersion: null }
}

export function resolveRelation(
  relation: RepositoryRelation,
  mods: readonly InstalledMod[],
  subject?: EvaluationSubject,
): RelationStatus {
  const evaluation = relationForSubject(relation, subject)
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

/** A subject's rows come from the backend, including empty overrides. */
export function resolveRelations(
  repo: RelationSubject,
  mods: readonly InstalledMod[],
  _repositories: readonly ModRepository[],
  releaseVersion?: string | null,
): RelationStatus[] {
  const subject = releaseVersion ? { repository_id: repo.id, tag: releaseVersion } : undefined
  return relationStatusesForSubject(subject, mods, () => relationsForRelease(repo, releaseVersion))
}

function relationIsUnmet(status: RelationStatus): boolean {
  return status.state !== 'ok' && status.state !== 'unknown' && status.state !== 'unassessed'
}

/** 只汇总已确认的问题，未知及等待评估的条目不阻断。 */
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

/** 具体原因供关系表、卡头提示和整链失败路径共用。 */
export const DEPENDENCY_ISSUE_TEXT = {
  missing: 'modChanges.relation.missing',
  disabled: 'modChanges.relation.disabled',
  version_mismatch: 'modChanges.relation.versionMismatch',
} as const

/** 卡头、页头筛选与关系表共用文案及色阶。 */
export const RELATION_STATUS = {
  relation_needs_action: { text: 'tools.relationNeedsAction', tone: 'critical' },
  relation_unconfirmed: { text: 'tools.relationUnconfirmed', tone: 'moderate' },
  relation_passed: { text: 'tools.relationPassed', tone: 'mild' },
} as const satisfies Record<string, { text: string; tone: TagTone }>

export const RELATION_CARD_KEYS = ['relation_needs_action', 'relation_unconfirmed'] as const
export type RelationCardKey = typeof RELATION_CARD_KEYS[number]

export function isRelationCardKey(key: string): key is RelationCardKey {
  return RELATION_CARD_KEYS.some(candidate => candidate === key)
}

function isExclusionConflict(status: RelationStatus): boolean {
  return status.state === 'conflict' && status.relation.kind === 'conflicts'
}

/** 兼容范围不符可以换版；只有确切的排斥关系才给单独的卸载入口。 */
export function conflictRemovalTarget(statuses: readonly RelationStatus[]): InstalledMod | undefined {
  return statuses.find(status => isExclusionConflict(status) && status.installedMod)?.installedMod
}

interface RelationStatusView {
  key: keyof typeof RELATION_STATUS
  text: string
  tone: 'critical' | 'moderate' | 'mild'
  reason?: string
}

export function relationStatusView(status: RelationStatus): RelationStatusView | undefined {
  const { state, relation } = status
  if (state === 'unassessed') return undefined
  if (state === 'unknown') {
    const reason = status.reason === 'duplicate_enabled_providers' ? 'modChanges.diagnostic.duplicateProviders'
      : status.reason === 'relation_metadata_unknown' ? 'modChanges.reason.metadata' : undefined
    return { key: 'relation_unconfirmed', ...RELATION_STATUS.relation_unconfirmed, reason }
  }
  if (state === 'ok') return { key: 'relation_passed', ...RELATION_STATUS.relation_passed }

  const issue = state === 'disabled' || state === 'version_mismatch' ? state
    : state === 'installable' && status.installedVersion ? 'version_mismatch' : 'missing'
  const reason = relation.kind === 'compatible' ? 'tools.incompatible'
    : state === 'conflict' || relation.kind === 'conflicts' ? 'modsOnline.conflict' : DEPENDENCY_ISSUE_TEXT[issue]
  return { key: 'relation_needs_action', ...RELATION_STATUS.relation_needs_action, reason }
}

/** 有权威修复根时优先交求解器，旧的精确候选仍可定位到仓库。 */
export function relationJumpTarget(status: RelationStatus | undefined): {
  id: string; repair?: DependencyRepairTarget; repository?: ModRepository
} | undefined {
  if (!status || !relationIsUnmet(status)) return undefined
  if (status.repair) return { id: status.repair.node_id, repair: status.repair }
  if (status.repository && (status.state === 'installable'
    || (status.relation.kind === 'compatible' && status.state === 'conflict'))) {
    return { id: status.repository.id, repository: status.repository }
  }
  return undefined
}

export type RelationCardTag = { key: RelationCardKey; dependency: RelationStatus; clickable: boolean }
  & typeof RELATION_STATUS[RelationCardKey]

/** 卡头只汇总已安装项：确认有问题优先，全部通过或尚未检查时不占标签位。 */
export function dependencyTagFor(statuses: readonly RelationStatus[], installed: boolean): RelationCardTag | undefined {
  if (!installed) return undefined
  const issues = unmetRelations(statuses)
  // 优先给整链入口，避免不可操作的问题把同卡可处理项的入口盖住。
  const dependency = issues.find(status => isExclusionConflict(status) && relationJumpTarget(status))
    ?? issues.find(status => relationJumpTarget(status))
    ?? issues[0] ?? statuses.find(status => status.state === 'unknown')
  const view = dependency && relationStatusView(dependency)
  return view && isRelationCardKey(view.key)
    ? { key: view.key, ...RELATION_STATUS[view.key], dependency, clickable: !!relationJumpTarget(dependency) }
    : undefined
}

/** 一次修复传一个 owner 根，由求解器检查它的全部依赖及冲突。 */
export function dependencyRepairStatus(statuses: readonly RelationStatus[]): RelationStatus | undefined {
  return statuses.find(status => !isExclusionConflict(status) && relationJumpTarget(status))
}
