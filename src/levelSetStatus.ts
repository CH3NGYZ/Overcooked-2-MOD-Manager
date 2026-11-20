import type { RelationSubject } from './modRelations'
import { aggregateLevelSetVersion, isSameLevelSet, mapUpdateState, mapVersionRef, type MapVersionRef } from './mapVersion.ts'
export { aggregateLevelSetVersion, isSameLevelSet, mapsForLevelSet, mapUpdateState, type MapVersionRef, type MapVersionOrder } from './mapVersion.ts'
import {
  conflictRemovalTarget, dependencyTagFor, relationsForMapRelease, relationStatusesForSubject, unmetRelations,
  type RelationStatus,
} from './modRelations.ts'
import type { InstalledMod, LevelSetInfo, MapInfo, ModRepository, RepositoryRelation } from './types'

/**
 * 关卡集（地图）的状态判定。
 *
 * ## 与 MOD 的区别
 *
 * 结构上很像，但**身份来源不同**：
 *
 * - MOD 的身份是 BepInPlugin 的 GUID，由 BepInEx 加载，能直接参与插件依赖关系；
 * - 关卡集的身份是 info 里的 `levelSetName + author`，由 DIYLevel 读取，
 *   与插件依赖体系无关。
 *
 * 所以两者共用 `compareVersions` 这类纯工具，但状态机各写一份 —— 硬把
 * 「本地关卡集」塞进 `InstalledMod` 会让每个使用点都要判断「这是哪种」。
 *
 * ## 为什么不能引入 Vue / Naive
 *
 * 本模块被 `tests/level-set-status.test.mjs` 用 `node --test` 直接 import
 * （Node 的 TS 类型擦除，没有 Vite 转译）。引入 `vue`、`naive-ui` 或
 * `i18n` 都会让这个测试在进程启动时就崩掉，所以这里只做纯计算：
 * 返回 **i18n key** 与色调，翻译与渲染留给视图层。
 */

/** 关卡集的展示状态。 */
export type LevelSetState =
  /** 已停用（在 `BepInEx/cache/modmanager_disabled_maps/` 里）。 */
  | 'disabled'
  /** info 解析失败：名称与版本都读不到，但目录仍然可用（能开关）。 */
  | 'unreadable'
  /** info 里的版本比仓库声明的版本低。 */
  | 'update_available'
  /** 与仓库版本一致。 */
  | 'latest'
  /** 本地版本比远端还高（作者的预发布，或仓库 tag 回退）。 */
  | 'ahead'
  /** 没有绑定任何地图仓库（本地导入的，或仓库已下线）。 */
  | 'unbound'
  /** 无法比较版本（任一侧缺失）。 */
  | 'unknown'

/**
 * 状态色调。
 *
 * 刻意不复用 `theme.ts` 的 `TagTone`：那个模块依赖 Vue，本模块必须能被
 * Node 直接引用。两个联合类型的取值保持一致（`theme.ts` 的
 * `tagColorForAny` 接受任意字符串），新增档位时要同时改两处。
 */
export type LevelSetTone = 'accent' | 'moderate' | 'muted' | 'critical'

/**
 * 从一个关卡集与其绑定的仓库版本，算出展示状态。
 *
 * `remote` 为 `undefined` 表示没有绑定（本地导入）或仓库还没刷新。
 */
export function levelSetState(
  set: Pick<LevelSetInfo, 'enabled' | 'version' | 'error'> & { installed_mapset_version?: string },
  remote?: MapVersionRef | null,
): LevelSetState {
  // 停用只影响游戏加载，仍须提醒已经安装的地图有更新。
  const order = remote && !set.error ? mapUpdateState(set.version, remote, set.installed_mapset_version) : 'unknown'
  if (!set.enabled) return order === 'update_available' ? order : 'disabled'
  if (set.error) return 'unreadable'
  if (!remote) return 'unbound'
  // 除 `unknown` 外，版本结论与 `LevelSetState` 的版本分支同名，直接透传
  if (order === 'unknown') return 'unknown'
  return order
}

/** Local state uses all matching sources; dependency binding remains first-source. */
export function levelSetAggregateState(
  set: Pick<LevelSetInfo, 'enabled' | 'error'>,
  aggregate: ReturnType<typeof aggregateLevelSetVersion>,
): LevelSetState {
  if (!set.enabled) return aggregate.order === 'update_available' ? 'update_available' : 'disabled'
  if (set.error) return 'unreadable'
  return aggregate.order === 'refreshing' ? 'unknown' : aggregate.order
}

/** 状态 → 展示用的 tag（与 `modsLocal.*` 的命名保持同一套语义）。 */
export const LEVEL_SET_STATE_TAGS: Record<LevelSetState, string> = {
  disabled: 'modsLocal.disabled',
  unreadable: 'levelSet.parseFailed',
  update_available: 'tools.update',
  latest: 'modsLocal.latest',
  // 「更高版本」不是「最新版」：本地比线上还新，结论与 MOD 页
  // 「存在更高版本，建议卸载」同档，必须分开说，否则用户会以为不用更新
  ahead: 'levelSet.higherVersion',
  unbound: 'modsLocal.unbound',
  unknown: 'tools.versionUnknown',
}

/** 状态 → 语义色调，与 MOD 侧共用同一档位。 */
export const LEVEL_SET_STATE_TONES: Record<LevelSetState, LevelSetTone> = {
  disabled: 'muted',
  unreadable: 'critical',
  update_available: 'moderate',
  latest: 'accent',
  ahead: 'accent',
  unbound: 'muted',
  unknown: 'muted',
}

/**
 * 找一个关卡集对应的仓库版本声明。
 *
 * 兼容旧单源调用，返回第一个直接匹配本地身份的声明。多源版本、动作和总览
 * 统一使用 aggregateLevelSetVersion；依赖与分组继续保留原绑定顺序。
 */
export function mapVersionForLevelSet(
  set: Pick<LevelSetInfo, 'name' | 'author'> & { name_zh?: string | null },
  refs: readonly MapVersionRef[],
): MapVersionRef | undefined {
  if (!set.name.trim()) return undefined
  return refs.find((ref) =>
    isSameLevelSet(
      { name: set.name, author: set.author, nameZh: set.name_zh },
      { name: ref.levelSetName, author: ref.author, nameZh: ref.nameZh },
    ))
}

/**
 * 把地图源列表压成版本声明表。
 *
 * **只有刷新到 Release 的地图才产生声明。** 没刷过的地图 `latest_tag` 为空，
 * 此时本地关卡集必须保持「未绑定」而不是被当成「最新版」—— 把「还没查」
 * 说成「已是最新」是错的结论，用户会因此错过真实更新。
 *
 * 源文档钉死的 `latest_mapset_version` 与最新 Release 的 info 版本号随声明一起带出：
 * 完整历史与同身份 info 的可信判定由 mapVersion 模块唯一实现。
 */
export function mapVersionRefs(maps: readonly MapInfo[]): MapVersionRef[] {
  return maps
    .filter((map) => !!map.latest_tag)
    .map((map) => {
      const projected = mapVersionRef(map)
      const ref: MapVersionRef = {
        tag: map.latest_tag as string,
        levelSetName: map.level_set_name,
        author: map.author,
      }
      // 钉死版本只在有值时带上，理由同下面的中文名：空字段只会给快照加噪音
      if (map.asset_versions?.length) ref.assetVersions = map.asset_versions
      const forced = projected.latestMapsetVersion?.trim()
      if (forced) ref.latestMapsetVersion = forced
      // 空的 info 版本不塞；缺失时由 mapVersion 依据完整共同版本表判断，
      // 不把包号或 tag 当作 info 版本。
      const infoVersion = projected.latestInfoVersion?.trim()
      if (infoVersion) ref.latestInfoVersion = infoVersion
      // 老地图源没有中文名时**不塞空字段**：下游按「两边都有才比较」判定，
      // 塞个 undefined 只会让相等性比较与快照多出一层噪音
      if (map.level_set_name_zh) ref.nameZh = map.level_set_name_zh
      return ref
    })
}

/**
 * 找本地关卡集绑定的地图源条目。
 *
 * 身份是 `level_set_name + level_set_name_zh + author`（与
 * `mapVersionForLevelSet` 同一规则）。
 * 集名为空（解析失败）时不做匹配：空字符串会与所有空名条目串在一起。
 */
export function mapForLevelSet(
  set: Pick<LevelSetInfo, 'name' | 'author'> & { name_zh?: string | null },
  maps: readonly MapInfo[],
): MapInfo | undefined {
  if (!set.name.trim()) return undefined
  return maps.find((map) =>
    isSameLevelSet(
      { name: set.name, author: set.author, nameZh: set.name_zh },
      { name: map.level_set_name, author: map.author, nameZh: map.level_set_name_zh },
    ))
}

/**
 * 反过来：找一张地图在本地的关卡集。
 *
 * 与 `mapForLevelSet` 是同一个身份判定，方向相反 —— 下载页的卡片主体是
 * 「地图源条目」，要知道它装没装、装的是哪一版，就得从地图找本地目录。
 * 地图声明的地图名为空时同样不匹配（解析失败的地图不能认领任意空名集）。
 */
export function levelSetForMap(
  map: Pick<MapInfo, 'level_set_name' | 'author'> & { level_set_name_zh?: string },
  sets: readonly LevelSetInfo[],
): LevelSetInfo | undefined {
  if (!map.level_set_name.trim()) return undefined
  return sets.find((set) =>
    isSameLevelSet(
      { name: map.level_set_name, author: map.author, nameZh: map.level_set_name_zh },
      { name: set.name, author: set.author, nameZh: set.name_zh },
    ))
}

/**
 * 一张地图「将要安装 / 当前展示」的 Release 版本。
 *
 * 优先用后端缓存的最新 tag（用户点安装取的就是它）；源的 `latest_mapset_version`
 * 是作者钉死的版本声明，只在没有 tag 时兜底。全都为空时返回 null ——
 * 调用方拿不到版本就只能用仓库级默认关系表。
 */
export function mapReleaseVersion(
  map: Pick<MapInfo, 'latest_tag' | 'latest_mapset_version' | 'latest_release'> | null | undefined,
): string | null {
  if (!map) return null
  const tag = map.latest_tag?.trim()
    || map.latest_release?.tag_name?.trim()
    || map.latest_release?.version?.trim()
    || map.latest_mapset_version?.trim()
  return tag || null
}

/** 省略版本时仅用于展示默认/最新版声明；`null` 表示详情没有选择。 */
export function mapRelations(map: MapInfo | null | undefined, version?: string | null): RepositoryRelation[] {
  return map ? relationsForMapRelease(map, version === undefined ? mapReleaseVersion(map) : version) : []
}

/** Maps supply only relation and release context to the shared evaluator. */
export function mapRelationSubject(map: MapInfo, version?: string | null): RelationSubject {
  return {
    id: map.id, name: map.name, name_i18n: map.name_i18n,
    cached_releases: map.cached_releases ?? (map.latest_release ? [map.latest_release] : []),
    relations: mapRelations(map, version),
  }
}

/** 一条框架依赖在本地的满足情况。 */
export interface FrameworkState {
  guid: string
  /** 依赖声明的版本要求（区间表达式原文，空表示不限）。 */
  version: string
  /** 本地已装版本；未装时为空。 */
  installedVersion: string | null
  /** 已装且版本落在依赖声明的区间内。 */
  satisfied: boolean
}

/** 仅在未评估时展示声明：按本地版本选取覆盖表，未绑定时用安装快照。 */
export function levelSetRelations(
  set: Pick<LevelSetInfo, 'name' | 'name_zh' | 'author' | 'version' | 'relations'>,
  maps: readonly MapInfo[],
): RepositoryRelation[] {
  const bound = mapForLevelSet(set, maps)
  return bound ? relationsForMapRelease(bound, set.version) : set.relations ?? []
}

/** 卡头与详情均按同一个已安装目录读取评估。 */
export function levelSetRelationStatuses(
  set: Parameters<typeof levelSetRelations>[0] & { dir_name?: string },
  maps: readonly MapInfo[],
  mods: readonly InstalledMod[],
  _repositories: readonly ModRepository[] = [],
): RelationStatus[] {
  return relationStatusesForSubject(set.dir_name ? { map_dir_name: set.dir_name } : undefined,
    mods, () => levelSetRelations(set, maps))
}

/**
 * 在线详情按选中 Release 评估；`null` 表示没有选中，不能借本地库存结论。
 * 省略版本的调用方按最新 Release；仅有地图集版本时只用于展示声明。
 */
export function mapRelationStatuses(
  map: MapInfo | null | undefined,
  mods: readonly InstalledMod[],
  _repositories: readonly ModRepository[] = [],
  version?: string | null,
): RelationStatus[] {
  if (!map) return []
  const tag = version === undefined ? map.latest_tag || map.latest_release?.tag_name : version
  return relationStatusesForSubject(tag ? { map_id: map.id, tag } : undefined, mods, () => mapRelations(map, version))
}

/**
 * 地图声明的框架依赖在本地的满足情况。
 *
 * 数据源是 Dashboard 已经扫好的 MOD 列表：地图页以前为了算这个让后端
 * 再全量扫一遍 `BepInEx/plugins`，加载时间白白翻倍。
 */
export function frameworkStates(
  map: MapInfo | null | undefined,
  mods: readonly InstalledMod[],
  repositories: readonly ModRepository[] = [],
): FrameworkState[] {
  return mapRelationStatuses(map, mods, repositories).map((status) => ({
    guid: status.relation.guid,
    version: status.relation.version?.trim() ?? '',
    installedVersion: status.installedVersion,
    satisfied: status.state === 'ok',
  }))
}

/**
 * 地图的框架依赖是否有缺失。
 *
 * `gameProbed` 为假（`game?.path` 为空）时返回 `null` —— 游戏都没探测到，
 * 本地 MOD 列表必然是空的，此时判「缺依赖」是把「不知道」说成「缺」。
 *
 * DIYLevel 与 DIYLevel Web Runtime 任一缺失都算依赖缺失，但这里**不写死
 * GUID**：判定完全由地图源里的 `required_frameworks` 声明驱动，GUID 只是
 * 数据。
 */
export function mapDependencyMissing(
  map: MapInfo | null | undefined,
  mods: readonly InstalledMod[],
  gameProbed: boolean,
  repositories: readonly ModRepository[] = [],
): boolean | null {
  if (!gameProbed) return null
  if (!map) return false
  const states = mapRelationStatuses(map, mods, repositories)
  if (unmetRelations(states).length) return true
  return states.some(state => state.state !== 'ok') ? null : false
}

/** 关卡集卡片状态区的一个标签。 */
export interface LevelSetCardTag {
  /** i18n key（视图层 `t()`）。 */
  text: string
  tone: LevelSetTone
  /** 关系摘要与版本结论分开。 */
  kind: 'state' | 'dependency'
  /** 版本标签可定位到绑定的在线地图。 */
  mapId?: string
  /** 关系标签的选中关系与是否有实际处理入口。 */
  dependency?: RelationStatus
  clickable?: boolean
}

/**
 * 正在刷新的对象（地图引擎 `useMapSources` 的实时状态）。
 *
 * 由视图层从单例里取出来传进来，本模块只做纯计算：这里不认 Vue、也不认
 * 「刷新进行中」这个全局标志，只回答「这张图 / 它所在的源现在是否在刷」。
 */
export interface LevelSetRefreshingHint {
  /**
   * 地图列表正在清空/重建（整源刷新的源树阶段）。
   *
   * 它会把共享的地图列表清空再重建，这段时间所有关卡集都查不到绑定 ——
   * 只看 id 集合的话卡片会集体退化成「未绑定」，比过期结论更误导。
   *
   * 只该覆盖源树阶段：列表填回来之后条目级状态就是准确的，再用整源标志
   * 盖住所有卡片，逐个返回的结果会被压成整批一起返回。
   */
  entryStateUnavailable?: boolean
  /** 源树声明顺序，用于不可比更新目标的稳定选择。 */
  sourceOrder?: readonly string[]
  /** 正在重抓 Release 的地图 id。 */
  maps: ReadonlySet<string>
  /**
   * 这一批里排队、还没轮到重抓的地图 id。
   *
   * 与 `maps` 同等对待：并发只有 4，排队的不显示「刷新中」会让人以为
   * 「只有那几张在查」。图标转不转另算，那是视图层的事。
   */
  queued?: ReadonlySet<string>
  /** 正在重拉文档的地图源 id。 */
  sources: ReadonlySet<string>
}

/** Local comparisons wait for sources, then only their own queued/in-flight API. */
export function levelSetIsRefreshing(
  set: LevelSetInfo,
  maps: readonly MapInfo[],
  refreshing?: LevelSetRefreshingHint,
): boolean {
  return aggregateLevelSetVersion(set, maps, refreshing).order === 'refreshing'
}

/**
 * 关卡集卡片状态区的完整标签列表。
 *
 * 先显示一条关系摘要，再显示独立的版本结论。停用地图同样可显示摘要，
 * 方便在启用前处理前置关系。
 *
 * 版本结论与「刷新中」互斥：刷新的那一刻旧的「有更新 / 最新版」就是过期
 * 结论，继续显示会让用户以为已经查完了（与 MOD 侧的 `replaceVersionConclusion`
 * 同一条规则）。三种情形都会点亮它：绑定地图自己在刷、它所在的源在重拉文档
 * （此时一个 Release 都还没回来）、以及整源刷新（列表正在重建，绑定暂时查不到）。
 */
export function levelSetCardTags(
  set: LevelSetInfo,
  maps: readonly MapInfo[],
  mods: readonly InstalledMod[],
  gameProbed: boolean,
  repositories: readonly ModRepository[] = [],
  refreshing?: LevelSetRefreshingHint,
): LevelSetCardTag[] {
  const bound = mapForLevelSet(set, maps)
  const tags: LevelSetCardTag[] = []

  if (gameProbed) {
    // 与详情使用同一个已安装目录和评估结论。
    const statuses = levelSetRelationStatuses(set, maps, mods, repositories)
    const tag = dependencyTagFor(statuses, true)
    if (tag) tags.push({ ...tag, kind: 'dependency' })
  }

  const aggregate = aggregateLevelSetVersion(set, maps, refreshing)
  const refreshingNow = aggregate.order === 'refreshing'
  const state = refreshingNow ? undefined : levelSetAggregateState(set, aggregate)
  tags.push({
    text: refreshingNow ? 'tools.refreshing' : LEVEL_SET_STATE_TAGS[state!],
    tone: refreshingNow ? 'muted' : LEVEL_SET_STATE_TONES[state!],
    kind: 'state',
    // 只要这个集绑定了地图，状态标签就允许跳回下载页并高亮那张卡 ——
    // 与 MOD 管理页一致：「最新版」和「有更新」都是「去下载页看看」的入口。
    // 未绑定（或没匹配上地图）没有目标，自然不可点。
    mapId: aggregate.primaryTarget?.mapId ?? aggregate.candidates[0]?.mapId ?? bound?.id,
  })
  return tags
}

/** 地图管理卡片上的一键动作类型。 */
export type LevelSetCardActionKind = 'update' | 'review-conflict'

/**
 * 地图管理卡片状态区的一键动作。
 *
 * 与 MOD 管理页的 `StatusActionSpec` 同一职责边界：这里只回答「显示什么按钮、
 * 点了去哪儿」，真正执行（预检 / 下载 / 导入 / 跳转）留在视图层 —— 本模块
 * 不能依赖 Vue、i18n 或任何 IPC。
 */
export interface LevelSetCardAction {
  kind: LevelSetCardActionKind
  /** 按钮文案的 i18n key。 */
  label: string
  tone: LevelSetTone
  /** `update` 要装的地图 id。 */
  mapId?: string
  /** `update` 捕获的实际安装 tag。 */
  tag?: string
  /** `review-conflict` 要跳转的 MOD 仓库 id。 */
  repositoryId?: string
  /** `review-conflict` 命中的 GUID，用来在提示里说清是哪一个依赖。 */
  guid?: string
}

/**
 * 冲突优先于更新；依赖修复有独立入口，不受这两个动作的优先级影响。
 */
export function levelSetCardAction(
  set: LevelSetInfo,
  maps: readonly MapInfo[],
  mods: readonly InstalledMod[],
  gameProbed: boolean,
  repositories: readonly ModRepository[] = [],
  refreshing?: LevelSetRefreshingHint,
): LevelSetCardAction | undefined {
  if (set.error || levelSetIsRefreshing(set, maps, refreshing)) return undefined
  const statuses = gameProbed && set.enabled
    ? levelSetRelationStatuses(set, maps, mods, repositories)
    : []
  const conflict = conflictRemovalTarget(statuses)
  if (conflict) {
    return {
      kind: 'review-conflict',
      label: 'levelSet.resolveConflict',
      tone: 'critical',
      repositoryId: conflict.repository_id,
      guid: conflict.guid,
    }
  }

  const aggregate = aggregateLevelSetVersion(set, maps, refreshing)
  const target = aggregate.primaryTarget
  if (target) {
    return {
      kind: 'update',
      label: 'common.action.update',
      tone: 'moderate',
      mapId: target.mapId,
      tag: target.tag,
    }
  }

  return undefined
}

/**
 * 关卡集的展示名：优先中文名，其次英文名，最后退回目录名。
 *
 * 与 MOD 的 `pickI18n` 不同：这里没有多语言表，只有「中英两个字段 + 目录名」
 * 三级回退，因为 info 里就只存了这两项。
 */
export function levelSetDisplayName(
  set: Pick<LevelSetInfo, 'name' | 'name_zh' | 'dir_name'>,
  locale: string,
): string {
  const preferChinese = locale.startsWith('zh')
  const chinese = set.name_zh?.trim()
  const english = set.name.trim()
  if (preferChinese && chinese) return chinese
  if (english) return english
  if (chinese) return chinese
  return set.dir_name
}

/**
 * 「未分组」的折叠键。
 *
 * 与 MOD 管理页的 `__ungrouped__` 同名同义，只是守卫常量放在这里，
 * 视图层不再自己抄一遍字符串。
 */
export const LEVEL_SET_UNGROUPED_KEY = '__ungrouped__'

/** 关卡集的展示分组：管理页上的一个折叠项。 */
export interface LevelSetGroup {
  /** 折叠状态用的稳定键：`group:<canonical 小写>`，或 `LEVEL_SET_UNGROUPED_KEY`。 */
  key: string
  /** 语言无关的规范名；未分组时为空串。 */
  canonical: string
  /**
   * 分组名的多语言表。
   *
   * 回退到本地 `author` 时为空表 —— 作者名不是翻译，视图层用
   * `pickI18n(labelI18n, canonical)` 就能同时覆盖「有译名」与「只有原名」两种。
   */
  labelI18n: Record<string, string>
}

/** 多语言表里第一个非空值；键顺序不稳定，只用来判断「声明过没有」。 */
function firstNonEmpty(map: Record<string, string> | undefined): string {
  if (!map) return ''
  for (const value of Object.values(map)) {
    if (value && value.trim()) return value.trim()
  }
  return ''
}

/**
 * 关卡集归到哪个分组（地图的分组语义就是**作者**）。
 *
 * ## 分组名的四级来源
 *
 *   1. 绑定地图的 `group_i18n`（源文档里的作者 / 分组声明，可多语言）；
 *   2. 绑定地图的单语镜像 `group`（手写或旧源文档可能只写单语）；
 *   3. 后端固化的分组快照 `set.map_group_i18n` / `set.map_group` —— 地图源
 *      被删掉之后绑定关系已经不在了，只有这份安装时留下的快照还记得
 *      「这张图归到哪个作者」；
 *   4. 本地 info 的 `author` —— 本地 ZIP 导入的地图没有线上条目，只剩这个。
 *
 * 顺序不能反过来：`author` 只是 AssetBundle 里的原始字符串，线上声明才是
 * 「这张图现在归到哪个分组」的权威（作者改名、调整分组都以线上为准）。
 * 这与 `levelSetCardTags` 判定版本时「以线上为准、本地兜底」是同一条口径。
 *
 * 最后一级是 `author`，所以任何能读出作者的地图都有分组；只有 info 解析
 * 失败（集名与作者都读不出来）才会落到「未分组」。
 */
export function levelSetGroup(
  set: Pick<LevelSetInfo, 'name' | 'author' | 'map_group' | 'map_group_i18n'>,
  maps: readonly MapInfo[],
): LevelSetGroup {
  const bound = mapForLevelSet(set, maps)
  // 绑定地图声明过分组就用它，否则用后端固化的快照（源文档已被删的情况）
  const groupI18n = firstNonEmpty(bound?.group_i18n)
    ? (bound?.group_i18n ?? {})
    : (set.map_group_i18n ?? {})
  const single = (bound?.group ?? '').trim() || (set.map_group ?? '').trim()
  /*
   * 规范名固定取 zh-CN：折叠键必须与语言无关，否则切语言会重建键、
   * 用户刚折叠的分组会突然展开（与 MOD 管理页的 modCategory 同一规则）。
   * 只声明了英文（或别的语言）也算「作者声明过分组」，不能退成作者名 ——
   * 否则同一个分组会随源文档补没补中文而换键。
   */
  const declared = (groupI18n['zh-CN'] ?? '').trim() || single || firstNonEmpty(groupI18n)
  const canonical = declared || (set.author || '').trim()
  if (!canonical) {
    return { key: LEVEL_SET_UNGROUPED_KEY, canonical: '', labelI18n: {} }
  }
  return {
    key: `group:${canonical.toLowerCase()}`,
    canonical,
    // 只有声明（多语言表或单语镜像）才有多语言文本；回退到 author 时没有
    labelI18n: declared ? groupI18n : {},
  }
}

/** 关卡集作者的中英两个展示名。 */
export interface LevelSetAuthorNames {
  zh: string
  en: string
}

/**
 * 作者的「中文名 / 英文名」（地图管理页详情用）。
 *
 * info 里只存一份 `author`，作者的双语展示名只可能来自分组声明：绑定地图的
 * `group_i18n`，或地图源被删后后端留下的 `set.map_group_i18n` 快照 ——
 * 地图的分组语义就是作者，源文档可以给作者声明多种语言。回退顺序与
 * `levelSetGroup` 同源：`group_i18n[语言]` → 单语镜像
 * （`group` / `set.map_group`）→ 本地 `author`，所以这里不需要第二套口径。
 *
 * 为什么某一种语言没声明时用另一种语言顶上，而不是留空：两行是同一个作者
 * 的两种写法，源文档常只声明 `zh-CN`（手工维护，很少有人补齐英文）。留空
 * 会让详情里冒出「—」，看起来像数据丢了；两行显示同一串，也与「只写了单语
 * `group`」时的表现一致。两种语言都没声明才回退 `author`。
 */
export function levelSetAuthorNames(
  set: Pick<LevelSetInfo, 'name' | 'author' | 'map_group' | 'map_group_i18n'>,
  maps: readonly MapInfo[],
): LevelSetAuthorNames {
  const bound = mapForLevelSet(set, maps)
  const i18n = firstNonEmpty(bound?.group_i18n)
    ? (bound?.group_i18n ?? {})
    : (set.map_group_i18n ?? {})
  const single = (bound?.group ?? '').trim() || (set.map_group ?? '').trim()
  const zhDeclared = (i18n['zh-CN'] ?? '').trim() || single
  const enDeclared = (i18n['en'] ?? '').trim() || single
  const author = (set.author || '').trim()
  return {
    zh: zhDeclared || enDeclared || author,
    en: enDeclared || zhDeclared || author,
  }
}
