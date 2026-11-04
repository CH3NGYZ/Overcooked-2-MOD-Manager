import { compareVersions } from './modStatus.ts'
import {
  dependencyTagFor, relationsForMapRelease, resolveRelation, resolveRelations,
  type RelationStatus,
} from './modRelations.ts'
import type { InstalledMod, LevelSetInfo, MapInfo, Repository } from './types'

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
  /** 已停用（在 `disabled/` 里）。 */
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

/** 地图仓库对某个关卡集的版本声明。 */
export interface MapVersionRef {
  /** Release tag（例如 `v0.73`）。 */
  tag: string
  /**
   * 源文档钉死的地图包版本（`[[maps]].latest_mapset_version`）。
   *
   * 非空时不做大小比较，只问「本地装的是不是这一版」—— 见 `mapUpdateState`。
   */
  latestMapsetVersion?: string | null
  /**
   * 最新 Release 里 **`info` 文件内的版本号**（来自 `MapInfo.latest_info_version`）。
   *
   * `latestMapsetVersion` 是地图包（Release）的版本，由作者定义；这一份是包内 info
   * 自己写的版本，两者平时相同，作者升级包版本却忘了改 info 时才不同 ——
   * 见 `mapUpdateState`（info 版本优先于包版本比较）。
   */
  latestInfoVersion?: string
  /** 关卡集名（与 info 的 `levelSetName` 比对）。 */
  levelSetName: string
  /** 中文名（与 info 的 `levelSetNameZH` 比对）；老地图源没有这个字段。 */
  nameZh?: string
  /** 作者（与 info 的 `author` 比对）。 */
  author: string
}

/**
 * 判断两个关卡集是否是「同一个地图」。
 *
 * 身份是 `author + levelSetName`，填了中文名（`levelSetNameZH`）时再加一段，
 * **不用 uid**：uid 是 `MD5(baseUID + version)`，每发一版就变，拿它当身份
 * 会把每次更新都认成新地图。
 *
 * 必须带中文名，否则同一个作者的两张不同地图会互相认领 —— 实测「胡闹糕中」
 * 与「胡闹达学」在 info 里都写 `GAODA / GAODA`，只靠英文集名与作者分不开。
 * 中文名**两边都有才比较**：老地图源不写这个字段，此时退回前两段，行为与
 * 加入中文名之前完全一致。
 *
 * 比较忽略大小写与首尾空白 —— 作者在不同工具里导出时大小写常不一致。
 */
export function isSameLevelSet(
  left: { name: string; author: string; nameZh?: string | null },
  right: { name: string; author: string; nameZh?: string | null },
): boolean {
  const normalize = (value: string) => value.trim().toLowerCase()
  if (normalize(left.name) !== normalize(right.name)) return false
  if (normalize(left.author) !== normalize(right.author)) return false
  const leftZh = normalize(left.nameZh ?? '')
  const rightZh = normalize(right.nameZh ?? '')
  return !leftZh || !rightZh || leftZh === rightZh
}

/**
 * 本地版本与线上声明比较后的结论。
 *
 * 与 `LevelSetState` 的版本分支同名同义（`update_available` / `latest` /
 * `ahead`），多一个 `unknown` 表示「比不出来」：管理页状态、下载页卡片状态、
 * 下载页按钮三处都从这里取结论，不允许各写一份比较。
 */
export type MapVersionOrder = 'latest' | 'update_available' | 'ahead' | 'unknown'

/**
 * 「本地版本 vs 线上声明」的唯一判定。
 *
 * ## info 版本 vs 包版本
 *
 * `latest_mapset_version`（以及 Release tag）描述的是**地图包**的版本，由作者定义；
 * 而本地扫描出来的 `LevelSetInfo.version` 读的是包内 **info 文件**自己写的
 * 版本号。两者平时相同，但作者升了包版本（tag `0.7`）却忘了改 info 里的
 * 版本号（还是 `0.69`）时，拿本地 `0.69` 去比 tag 会永远显示「可更新」。
 * 所以源文档声明了 `latest_info_version`（最新 Release 的 info 版本）时，
 * 先拿它与本地 info 版本比 —— 同一套版本空间，结论才是「装的到底是不是最新」。
 * （曾试过比 info 内容哈希，已废弃：info 版本本身就在 `version` 里，不需要
 * 额外算哈希，作者改不改版本号都能直接比。）
 *
 * 判定顺序：
 *
 * 1. `latestInfoVersion` 与本地版本都非空 → 按大小比 info 版本（相等 = 最新）；
 * 2. 否则 `latest_mapset_version` 非空 → 只比等值（见下）；
 * 3. 否则与 Release tag 比大小。
 *
 * ## 钉死版本（`latest_mapset_version`）
 *
 * 源文档写了 `latest_mapset_version` 时规则换成**等值**：作者已经指定「这就是最新版」，
 * 不再比大小 —— 资产名里的版本号是手写的，`0.7` 按数字段比反而小于 `0.69`，
 * 比大小会得出反的结论；本地更高也只说明作者要用户回退到这一版。
 * 等值仍走 `compareVersions`：`v0.7`、`0.7.0` 与源文档写的 `0.7` 是同一版，
 * 字符串直比会让用户被反复提示更新。
 *
 * 本地版本为空或不可解析时：钉死版本模式下算 `update_available`（没装上指定
 * 版本就是没到位），普通模式仍是 `unknown`（不把「不知道」说成「有更新」）。
 */
export function mapUpdateState(
  localVersion: string | null | undefined,
  remote: Pick<MapVersionRef, 'tag' | 'latestMapsetVersion' | 'latestInfoVersion'> | null | undefined,
): MapVersionOrder {
  if (!remote) return 'unknown'
  const infoVersion = remote.latestInfoVersion?.trim()
  const local = localVersion?.trim()
  if (infoVersion && local) {
    const infoOrder = compareVersions(local, infoVersion)
    // 比不出来（info 版本号不合法）就到此为止：再退回包版本比较会拿两个不同
    // 版本空间的数字互相比，结论比「不知道」更容易骗人
    if (infoOrder === null) return 'unknown'
    if (infoOrder < 0) return 'update_available'
    if (infoOrder > 0) return 'ahead'
    return 'latest'
  }
  const forced = remote.latestMapsetVersion?.trim()
  if (forced) {
    return compareVersions(localVersion, forced) === 0 ? 'latest' : 'update_available'
  }
  const order = compareVersions(localVersion, remote.tag)
  if (order === null) return 'unknown'
  if (order < 0) return 'update_available'
  if (order > 0) return 'ahead'
  return 'latest'
}

/**
 * 从一个关卡集与其绑定的仓库版本，算出展示状态。
 *
 * `remote` 为 `undefined` 表示没有绑定（本地导入）或仓库还没刷新。
 */
export function levelSetState(
  set: Pick<LevelSetInfo, 'enabled' | 'version' | 'error'>,
  remote?: MapVersionRef | null,
): LevelSetState {
  // 停用优先：用户关掉的东西不该显示「可更新」，那会催他去更新一个不用的地图
  if (!set.enabled) return 'disabled'
  if (set.error) return 'unreadable'
  if (!remote) return 'unbound'
  const order = mapUpdateState(set.version, remote)
  // 除 `unknown` 外，版本结论与 `LevelSetState` 的版本分支同名，直接透传
  if (order === 'unknown') return 'unknown'
  return order
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
 * 返回第一个身份匹配的仓库的 tag。多个仓库声明同一个关卡集属于配置错误，
 * 这里不猜哪个「更对」，按传入顺序取第一个 —— 与 MOD 侧 `repositoryForMod`
 * 的行为保持一致。
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
 * 怎么比由 `mapUpdateState` 一处决定，调用方不需要知道这次是比 info 版本、
 * 比等值还是比大小。
 */
export function mapVersionRefs(maps: readonly MapInfo[]): MapVersionRef[] {
  return maps
    .filter((map) => !!map.latest_tag)
    .map((map) => {
      const ref: MapVersionRef = {
        tag: map.latest_tag as string,
        levelSetName: map.level_set_name,
        author: map.author,
      }
      // 钉死版本只在有值时带上，理由同下面的中文名：空字段只会给快照加噪音
      const forced = map.latest_mapset_version?.trim()
      if (forced) ref.latestMapsetVersion = forced
      // 最新 Release 的 info 版本同理：空值（老地图源 / 块里没写）不塞，
      // 判定侧按「有值才优先」处理，缺了自动退回包版本比较
      const infoVersion = map.latest_info_version?.trim()
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
 * 把一张地图投影成 `Repository`，仅供关系判定使用。
 *
 * 为什么需要投影：地图的框架依赖与 MOD 的插件依赖是**同一套模型**
 * （`relations` + `required_frameworks`），判定逻辑已经写在 `modRelations.ts`
 * 里。地图不是 `Repository`，但只要有 `relations` 就能复用那条管线，
 * 不需要为地图再写一份依赖判定。
 *
 * 只填关系判定真正会读的字段，其余给安全默认值；新增字段时如果关系判定
 * 会读它，记得同步这里（`mapRelationRepository` 是唯一的投影入口）。
 */
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

/**
 * 把一张地图投影成 `Repository`，仅供关系判定使用。
 *
 * 为什么需要投影：地图的框架依赖与 MOD 的插件依赖是**同一套模型**
 * （`relations` + `required_frameworks`），判定逻辑已经写在 `modRelations.ts`
 * 里。地图不是 `Repository`，但只要有 `relations` 就能复用那条管线，
 * 不需要为地图再写一份依赖判定。
 *
 * `version` 是这次要判定/安装的 Release 版本：覆盖表（`release_relations`）
 * 按它选表，不传则用当前最新版本（`mapReleaseVersion`）。关系已经在这里
 * 定好，调用方不用再把版本传给 `resolveRelations` —— 传了也只会因为投影里
 * 没有覆盖表而原样返回。
 *
 * 只填关系判定真正会读的字段，其余给安全默认值；新增字段时如果关系判定
 * 会读它，记得同步这里（`mapRelationRepository` 是唯一的投影入口）。
 */
export function mapRelationRepository(
  map: MapInfo,
  version?: string | null,
): Repository {
  return {
    id: map.id,
    name: map.name,
    name_i18n: map.name_i18n,
    provider: map.provider,
    username: map.username,
    repository: map.repository,
    // 下面这些字段关系判定不读，给安全默认值
    download_mode: 'release',
    branch: '',
    file_path: '',
    channel: 'all',
    asset_rules: [],
    install_target: '',
    note: '',
    plugin_guid: '',
    local_product_name: '',
    local_original_filename: '',
    version_regex: '',
    max_release_count: 10,
    download_threads: 0,
    enabled: map.enabled,
    builtin: map.builtin,
    // 契约里 `get_maps` 会填全部缓存 Release；老快照或手写测试对象可能没有
    cached_releases: map.cached_releases ?? (map.latest_release ? [map.latest_release] : []),
    relations: relationsForMapRelease(map, version ?? mapReleaseVersion(map)),
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

/**
 * 关卡集的关系声明解析结果（含「可安装来源」）。
 *
 * 声明来源与依赖表完全同一条链：绑定的地图还在配置里就用它的在线声明，
 * 地图源下线（或本地导入的图没有线上条目）才退回安装清单里的快照
 * （`set.relations`）。卡片标签与依赖表因此不会对同一张图说两套话 ——
 * 早期卡片只按「绑定的在线地图」判，未绑定的图卡片上没有依赖提醒，
 * 详情里却列着一条缺依赖。
 */
export function levelSetRelationStatuses(
  set: Pick<LevelSetInfo, 'name' | 'author' | 'version' | 'relations'>,
  maps: readonly MapInfo[],
  mods: readonly InstalledMod[],
  repositories: readonly Repository[] = [],
): RelationStatus[] {
  const bound = mapForLevelSet(set, maps)
  // 已安装的图按**本地版本**取覆盖表：卡片说的是「这一版跑不跑得起来」，
  // 不是「最新版要不要什么」。
  if (bound) return mapRelationStatuses(bound, mods, repositories, set.version)
  return (set.relations ?? []).map((relation) => resolveRelation(relation, mods, repositories))
}

/**
 * 地图声明的关系在本地的解析结果（含「可安装来源」）。
 *
 * `repositories` 必须传：地图声明的前置就是仓库里的 MOD（DIYLevel、
 * Web Runtime 都有 `plugin_guid`），不传的话每条依赖都会被判成「没有来源」，
 * 地图页与 MOD 页就会对同一件事给出不同结论 —— 这正是两边口径要统一的地方。
 *
 * `version` 是这次要安装的 Release 版本（不传用最新版），用于选版本覆盖表。
 */
export function mapRelationStatuses(
  map: MapInfo | null | undefined,
  mods: readonly InstalledMod[],
  repositories: readonly Repository[] = [],
  version?: string | null,
): RelationStatus[] {
  if (!map) return []
  return resolveRelations(mapRelationRepository(map, version), mods, repositories)
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
  repositories: readonly Repository[] = [],
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
  repositories: readonly Repository[] = [],
): boolean | null {
  if (!gameProbed) return null
  if (!map) return false
  return frameworkStates(map, mods, repositories).some((state) => !state.satisfied)
}

/** 关卡集卡片状态区的一个标签。 */
export interface LevelSetCardTag {
  /** i18n key（视图层 `t()`）。 */
  text: string
  tone: LevelSetTone
  /** 这个标签来自哪一类判定：依赖缺失与版本结论是两件事。 */
  kind: 'state' | 'dependency'
  /**
   * 「有更新」→ 跳到下载页并高亮那张卡；
   * 「缺依赖」→ 跳到 MOD 安装页并高亮能提供这个依赖的仓库卡片。
   */
  mapId?: string
  /** 依赖的安装入口：能提供满足区间版本的仓库卡片 id（只有「缺依赖」会带）。 */
  repositoryId?: string
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

/**
 * 关卡集卡片状态区的完整标签列表。
 *
 * 顺序与 MOD 卡片一致：先「缺依赖」这类关系提醒，再版本结论 ——
 * 关系提醒说的是「装上能不能跑」，比「有没有新版」更该先被看到。
 *
 * 依赖提醒与 MOD 卡片共用 `dependencyTagFor` 一条规则：文案是「缺依赖」，
 * 线上有可安装来源就带上仓库 id（标签可点，跳过去就能补上），没有来源就
 * 只是纯文字。只在**能判定**（游戏已探测）时出现；停用的地图仍然显示，
 * 因为用户重新启用前就该知道它还缺东西。
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
  repositories: readonly Repository[] = [],
  refreshing?: LevelSetRefreshingHint,
): LevelSetCardTag[] {
  const bound = mapForLevelSet(set, maps)
  const tags: LevelSetCardTag[] = []

  if (gameProbed) {
    // 与依赖表同一条声明链：绑定的在线地图优先，没有就退回本地快照
    const dependency = dependencyTagFor(
      levelSetRelationStatuses(set, maps, mods, repositories),
      true,
    )
    if (dependency) {
      tags.push({
        text: dependency.text,
        tone: dependency.tone,
        kind: 'dependency',
        repositoryId: dependency.repositoryId,
      })
    }
  }

  const state = levelSetState(set, mapVersionForLevelSet(set, mapVersionRefs(maps)))
  const refreshingNow = !!refreshing && (
    refreshing.entryStateUnavailable === true
    || (!!bound && (
      refreshing.maps.has(bound.id)
      || !!refreshing.queued?.has(bound.id)
      || (!!bound.source_id && refreshing.sources.has(bound.source_id))
    ))
  )
  tags.push({
    text: refreshingNow ? 'tools.refreshing' : LEVEL_SET_STATE_TAGS[state],
    tone: refreshingNow ? 'muted' : LEVEL_SET_STATE_TONES[state],
    kind: 'state',
    // 只要这个集绑定了地图，状态标签就允许跳回下载页并高亮那张卡 ——
    // 与 MOD 管理页一致：「最新版」和「有更新」都是「去下载页看看」的入口。
    // 未绑定（或没匹配上地图）没有目标，自然不可点。
    mapId: bound?.id,
  })
  return tags
}

/** 地图管理卡片上的一键动作类型。 */
export type LevelSetCardActionKind = 'update' | 'install-dependency' | 'review-conflict'

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
  /** `install-dependency` / `review-conflict` 要跳转的 MOD 仓库 id。 */
  repositoryId?: string
  /** `review-conflict` 命中的 GUID，用来在提示里说清是哪一个依赖。 */
  guid?: string
}

/**
 * 一张地图卡片当前该给哪个一键动作。
 *
 * 优先序与 MOD 管理页一致（`statusActions.ts` 的 `ACTION_PRIORITY`）：
 * 冲突 > 可更新 > 依赖待补。理由相同 —— 冲突会让整张图跑不起来，比「有没有
 * 新版」更紧急；而「去安装」只是跳转，用户随时能点，不该盖住真正的修复入口。
 *
 * 最后那一档原来只叫「缺依赖」，现在带上了「可更新 / 可安装」（见
 * `modRelations.dependencyTagFor`）：三者都是同一个入口——跳到能提供依赖的
 * 仓库装/升级那个插件，所以按钮文案与色调统一按中档处理。
 *
 * 不给出动作的情况：
 *   · 停用 / info 解析失败：状态本身就不完整，先让用户处理卡片的开关；
 *   · 可更新但没有绑定到在线地图：没有可下载的来源；
 *   · 线上没有满足区间的仓库（真·缺依赖）：按钮点了没反应，比没有按钮更糟
 *     （沿用 `modRelations.dependencyTagFor` 只给可安装项 `repositoryId` 的规则）。
 */
export function levelSetCardAction(
  set: LevelSetInfo,
  maps: readonly MapInfo[],
  mods: readonly InstalledMod[],
  gameProbed: boolean,
  repositories: readonly Repository[] = [],
): LevelSetCardAction | undefined {
  if (!set.enabled || set.error) return undefined
  const statuses = gameProbed
    ? levelSetRelationStatuses(set, maps, mods, repositories)
    : []
  const conflict = statuses.find((status) => status.state === 'conflict' && status.installedMod)
  if (conflict?.installedMod) {
    return {
      kind: 'review-conflict',
      label: 'levelSet.resolveConflict',
      tone: 'critical',
      repositoryId: conflict.installedMod.repository_id,
      guid: conflict.relation.guid,
    }
  }

  const bound = mapForLevelSet(set, maps)
  const state = levelSetState(set, mapVersionForLevelSet(set, mapVersionRefs(maps)))
  if (state === 'update_available' && bound?.latest_tag) {
    return {
      kind: 'update',
      label: 'dialogs.update',
      tone: 'moderate',
      mapId: bound.id,
    }
  }

  const dependency = dependencyTagFor(statuses, true)
  if (dependency?.kind === 'installable' && dependency.repositoryId) {
    return {
      kind: 'install-dependency',
      label: 'overview.browse',
      tone: 'moderate',
      repositoryId: dependency.repositoryId,
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

/**
 * 关卡集归到哪个分组（地图的分组语义就是**作者**）。
 *
 * ## 为什么线上优先、本地兜底
 *
 * 分组名有三个来源，权威性递减：
 *
 *   1. 绑定地图的 `group_i18n`（源文档里的作者 / 分组声明，可多语言）；
 *   2. 绑定地图的单语镜像 `group`（手写或旧源文档可能只写单语）；
 *   3. 本地 info 的 `author` —— 本地 ZIP 导入的地图没有线上条目，只剩这个。
 *
 * 顺序不能反过来：`author` 只是 AssetBundle 里的原始字符串，线上声明才是
 * 「这张图现在归到哪个分组」的权威（作者改名、调整分组都以线上为准）。
 * 这与 `levelSetCardTags` 判定版本时「以线上为准、本地兜底」是同一条口径。
 *
 * 三级都空（info 解析失败且未绑定任何地图）才是「未分组」。
 */
export function levelSetGroup(
  set: Pick<LevelSetInfo, 'name' | 'author'>,
  maps: readonly MapInfo[],
): LevelSetGroup {
  const bound = mapForLevelSet(set, maps)
  // 规范名固定取 zh-CN：折叠键必须与语言无关，否则切语言会重建键、
  // 用户刚折叠的分组会突然展开（与 MOD 管理页的 modCategory 同一规则）
  const declared = (bound?.group_i18n?.['zh-CN'] ?? '').trim() || (bound?.group ?? '').trim()
  const canonical = declared || (set.author || '').trim()
  if (!canonical) {
    return { key: LEVEL_SET_UNGROUPED_KEY, canonical: '', labelI18n: {} }
  }
  return {
    key: `group:${canonical.toLowerCase()}`,
    canonical,
    // 只有线上声明才有多语言表；回退到 author 时没有可翻译的文本
    labelI18n: declared ? (bound?.group_i18n ?? {}) : {},
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
 * info 里只存一份 `author`，作者的双语展示名只可能来自绑定地图的
 * `group_i18n` —— 地图的分组语义就是作者，源文档可以给作者声明多种语言。
 * 回退顺序与 `levelSetGroup` 同源：`group_i18n[语言]` → 单语镜像 `group`
 * → 本地 `author`，所以这里不需要第二套口径。
 *
 * 为什么某一种语言没声明时用另一种语言顶上，而不是留空：两行是同一个作者
 * 的两种写法，源文档常只声明 `zh-CN`（手工维护，很少有人补齐英文）。留空
 * 会让详情里冒出「—」，看起来像数据丢了；两行显示同一串，也与「只写了单语
 * `group`」时的表现一致。两种语言都没声明才回退 `author`。
 */
export function levelSetAuthorNames(
  set: Pick<LevelSetInfo, 'name' | 'author'>,
  maps: readonly MapInfo[],
): LevelSetAuthorNames {
  const bound = mapForLevelSet(set, maps)
  const i18n = bound?.group_i18n ?? {}
  const single = (bound?.group ?? '').trim()
  const zhDeclared = (i18n['zh-CN'] ?? '').trim() || single
  const enDeclared = (i18n['en'] ?? '').trim() || single
  const author = (set.author || '').trim()
  return {
    zh: zhDeclared || enDeclared || author,
    en: enDeclared || zhDeclared || author,
  }
}

// ---------------------------------------------------------------------------
// 下载页卡片状态
// ---------------------------------------------------------------------------

/**
 * 地图下载页卡片的状态标签。
 *
 * ## 为什么和「管理页」的状态不是同一套
 *
 * 管理页的主体是**本地关卡集**，要回答「这个已经装在盘上的集有没有新版」，
 * 所以用 `LevelSetState`（含停用、解析失败、本地更高）。
 *
 * 下载页的主体是**地图源条目**，要回答 MOD 安装页那个问题：
 * 「这东西我装了没 / 该点安装还是重装」。同一个字段在两种页面语义不同，
 * 硬复用会让「已停用」这种纯本地状态出现在安装页，也会让「未安装」在管理
 * 页无意义 —— 因此这里单列一套，但文案与色调全部复用 MOD 侧已有的 key，
 * 保证两张页面的同一状态长得一样。
 */
export type MapCardState =
  /** 本地没有，线上有 Release：可以装。 */
  | 'not_installed'
  /** 本地已装，但线上还没刷到 Release：知道装了，不知道是不是最新。 */
  | 'installed'
  /** 线上版本比本地高。 */
  | 'update_available'
  /** 与线上一致（或本地更高，例如作者预发布）。 */
  | 'latest'
  /** 刷新失败 / 版本无法比较：结论不可信，不能说「已是最新」。 */
  | 'cannot_check'
  /** 本地没装，线上也还没刷到：还不知道能不能装。 */
  | 'not_refreshed'

/** 状态 → i18n key。全部复用 MOD 侧已有条目（`cannotCheckUpdate` 除外）。 */
export const MAP_CARD_STATE_TAGS: Record<MapCardState, string> = {
  not_installed: 'overview.notInstalled',
  installed: 'modsLocal.installed',
  update_available: 'tools.update',
  latest: 'modsLocal.latest',
  // 与 MOD 的「刷新失败 → 无法检测更新」同一句结论，但在地图域里另有措辞：
  // 地图刷新失败是源的问题，用户要重试的是「刷新」，不是重装
  cannot_check: 'levelSet.cannotCheckUpdate',
  not_refreshed: 'modsOnline.notRefreshed',
}

/** 状态 → 语义色调，与 MOD 侧同一档位。 */
export const MAP_CARD_STATE_TONES: Record<MapCardState, LevelSetTone> = {
  not_installed: 'muted',
  installed: 'accent',
  update_available: 'moderate',
  latest: 'accent',
  cannot_check: 'moderate',
  not_refreshed: 'muted',
}

/**
 * 算出下载页卡片的本地状态。
 *
 * 判定顺序刻意这样排：
 *
 * 1. `last_error` 优先 —— 刷新失败的缓存不能拿来下结论（AGENTS.md 里
 *    「刷新失败的仓库缓存不能确认最新版或有更新」同一条规则）；
 * 2. 没有 `latest_tag` 时不比版本，只说「装了」或「还没查到」；
 * 3. 版本比较不出结果（info 解析失败、版本号不合法）时给「无法检测更新」，
 *    而不是把空字符串当成「最新版」。
 *
 * 版本结论本身走 `mapUpdateState`：先按最新 Release 的 info 版本比（同一套
 * 版本空间），再按源文档钉死的 `latest_mapset_version` 比等值，最后才比 tag；
 * 下载页与管理页因此不会对同一张图给出两个结论。
 */
export function mapCardState(
  map: Pick<
    MapInfo,
    'level_set_name' | 'author' | 'latest_tag' | 'latest_mapset_version' | 'latest_info_version'
    | 'last_error'
  >,
  sets: readonly LevelSetInfo[],
): MapCardState {
  if (map.last_error) return 'cannot_check'
  const set = levelSetForMap(map, sets)
  if (!map.latest_tag) return set ? 'installed' : 'not_refreshed'
  if (!set) return 'not_installed'
  const order = mapUpdateState(set.version, {
    tag: map.latest_tag,
    latestMapsetVersion: map.latest_mapset_version,
    latestInfoVersion: map.latest_info_version ?? undefined,
  })
  if (order === 'unknown') return 'cannot_check'
  return order === 'update_available' ? 'update_available' : 'latest'
}

/** 下载页卡片状态区的渲染值。 */
export interface MapCardStatus {
  key: MapCardState
  /** i18n key（视图层 `t()`）。 */
  text: string
  tone: LevelSetTone
}

/** `mapCardState` + 文案 + 色调，视图层直接用这一份。 */
export function mapCardStatus(
  map: Pick<
    MapInfo,
    'level_set_name' | 'author' | 'latest_tag' | 'latest_mapset_version' | 'latest_info_version'
    | 'last_error'
  >,
  sets: readonly LevelSetInfo[],
): MapCardStatus {
  const key = mapCardState(map, sets)
  return { key, text: MAP_CARD_STATE_TAGS[key], tone: MAP_CARD_STATE_TONES[key] }
}

/** 下载页卡片动作，含义与 `OnlineCard` 的 `CardActionKind` 对齐。 */
export type MapCardAction = 'install' | 'reinstall' | 'update' | 'refresh'

/**
 * 卡片该给哪个按钮 —— 与 MOD 安装页同一套规则：
 *
 * - 没 Release 或刷新失败：只能「刷新」（没有 tag 无从安装）；
 * - 本地没装：安装；
 * - 本地版本更低：更新；
 * - 已经装了（同版本、本地更高、或版本比不了）：重装 —— 重装是幂等的，
 *   而「安装」在已装状态下会让用户以为会新增一份。
 */
export function mapCardAction(
  map: Pick<
    MapInfo,
    'level_set_name' | 'author' | 'latest_tag' | 'latest_mapset_version' | 'latest_info_version'
    | 'last_error'
  >,
  sets: readonly LevelSetInfo[],
): MapCardAction {
  if (map.last_error || !map.latest_tag) return 'refresh'
  const set = levelSetForMap(map, sets)
  if (!set) return 'install'
  const order = mapUpdateState(set.version, {
    tag: map.latest_tag,
    latestMapsetVersion: map.latest_mapset_version,
    latestInfoVersion: map.latest_info_version ?? undefined,
  })
  return order === 'update_available' ? 'update' : 'reinstall'
}
