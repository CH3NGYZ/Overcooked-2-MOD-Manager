import { compareVersions } from './modStatus.ts'
import {
  dependencyTagFor, resolveRelation, resolveRelations, type RelationStatus,
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
  /** 关卡集名（与 info 的 `levelSetName` 比对）。 */
  levelSetName: string
  /** 作者（与 info 的 `author` 比对）。 */
  author: string
}

/**
 * 判断两个关卡集是否是「同一个地图」。
 *
 * 用 `author + levelSetName`，**不用 uid**：uid 是 `MD5(baseUID + version)`，
 * 每发一版就变，拿它当身份会把每次更新都认成新地图。
 *
 * 比较忽略大小写与首尾空白 —— 作者在不同工具里导出时大小写常不一致。
 */
export function isSameLevelSet(
  left: { name: string; author: string },
  right: { name: string; author: string },
): boolean {
  const normalize = (value: string) => value.trim().toLowerCase()
  return normalize(left.name) === normalize(right.name) && normalize(left.author) === normalize(right.author)
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
  const order = compareVersions(set.version, remote.tag)
  if (order === null) return 'unknown'
  if (order < 0) return 'update_available'
  if (order > 0) return 'ahead'
  return 'latest'
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
  set: Pick<LevelSetInfo, 'name' | 'author'>,
  refs: readonly MapVersionRef[],
): MapVersionRef | undefined {
  if (!set.name.trim()) return undefined
  return refs.find((ref) => isSameLevelSet(set, { name: ref.levelSetName, author: ref.author }))
}

/**
 * 把地图源列表压成版本声明表。
 *
 * **只有刷新到 Release 的地图才产生声明。** 没刷过的地图 `latest_tag` 为空，
 * 此时本地关卡集必须保持「未绑定」而不是被当成「最新版」—— 把「还没查」
 * 说成「已是最新」是错的结论，用户会因此错过真实更新。
 */
export function mapVersionRefs(maps: readonly MapInfo[]): MapVersionRef[] {
  return maps
    .filter((map) => !!map.latest_tag)
    .map((map) => ({
      tag: map.latest_tag as string,
      levelSetName: map.level_set_name,
      author: map.author,
    }))
}

/**
 * 找本地关卡集绑定的地图源条目。
 *
 * 身份是 `level_set_name + author`（与 `mapVersionForLevelSet` 同一规则）。
 * 集名为空（解析失败）时不做匹配：空字符串会与所有空名条目串在一起。
 */
export function mapForLevelSet(
  set: Pick<LevelSetInfo, 'name' | 'author'>,
  maps: readonly MapInfo[],
): MapInfo | undefined {
  if (!set.name.trim()) return undefined
  return maps.find((map) =>
    isSameLevelSet(set, { name: map.level_set_name, author: map.author }))
}

/**
 * 反过来：找一张地图在本地的关卡集。
 *
 * 与 `mapForLevelSet` 是同一个身份判定，方向相反 —— 下载页的卡片主体是
 * 「地图源条目」，要知道它装没装、装的是哪一版，就得从地图找本地目录。
 * 地图声明的地图名为空时同样不匹配（解析失败的地图不能认领任意空名集）。
 */
export function levelSetForMap(
  map: Pick<MapInfo, 'level_set_name' | 'author'>,
  sets: readonly LevelSetInfo[],
): LevelSetInfo | undefined {
  if (!map.level_set_name.trim()) return undefined
  return sets.find((set) =>
    isSameLevelSet({ name: map.level_set_name, author: map.author }, set))
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
export function mapRelationRepository(map: MapInfo): Repository {
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
    relations: (map.required_frameworks ?? []).map((framework) => ({
      kind: 'requires' as const,
      guid: framework.guid,
      min_version: framework.min_version ?? '',
      max_version: '',
    })),
  }
}

/** 一条框架依赖在本地的满足情况。 */
export interface FrameworkState {
  guid: string
  /** 依赖声明的最低版本（空表示不限）。 */
  minVersion: string
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
  set: Pick<LevelSetInfo, 'name' | 'author' | 'relations'>,
  maps: readonly MapInfo[],
  mods: readonly InstalledMod[],
  repositories: readonly Repository[] = [],
): RelationStatus[] {
  const bound = mapForLevelSet(set, maps)
  if (bound) return mapRelationStatuses(bound, mods, repositories)
  return (set.relations ?? []).map((relation) => resolveRelation(relation, mods, repositories))
}

/**
 * 地图声明的关系在本地的解析结果（含「可安装来源」）。
 *
 * `repositories` 必须传：地图声明的前置就是仓库里的 MOD（DIYLevel、
 * Web Runtime 都有 `plugin_guid`），不传的话每条依赖都会被判成「没有来源」，
 * 地图页与 MOD 页就会对同一件事给出不同结论 —— 这正是两边口径要统一的地方。
 */
export function mapRelationStatuses(
  map: MapInfo | null | undefined,
  mods: readonly InstalledMod[],
  repositories: readonly Repository[] = [],
): RelationStatus[] {
  if (!map) return []
  return resolveRelations(mapRelationRepository(map), mods, repositories)
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
    minVersion: status.relation.min_version?.trim() ?? '',
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
   * 整源刷新进行中（下载页的「刷新全部」）。
   *
   * 它会把共享的地图列表清空再重建，这段时间所有关卡集都查不到绑定 ——
   * 只看 id 集合的话卡片会集体退化成「未绑定」，比过期结论更误导。
   */
  all?: boolean
  /** 正在重抓 Release 的地图 id。 */
  maps: ReadonlySet<string>
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
    refreshing.all === true
    || (!!bound && (
      refreshing.maps.has(bound.id)
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
 */
export function mapCardState(
  map: Pick<MapInfo, 'level_set_name' | 'author' | 'latest_tag' | 'last_error'>,
  sets: readonly LevelSetInfo[],
): MapCardState {
  if (map.last_error) return 'cannot_check'
  const set = levelSetForMap(map, sets)
  if (!map.latest_tag) return set ? 'installed' : 'not_refreshed'
  if (!set) return 'not_installed'
  const order = compareVersions(set.version, map.latest_tag)
  if (order === null) return 'cannot_check'
  return order < 0 ? 'update_available' : 'latest'
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
  map: Pick<MapInfo, 'level_set_name' | 'author' | 'latest_tag' | 'last_error'>,
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
  map: Pick<MapInfo, 'level_set_name' | 'author' | 'latest_tag' | 'last_error'>,
  sets: readonly LevelSetInfo[],
): MapCardAction {
  if (map.last_error || !map.latest_tag) return 'refresh'
  const set = levelSetForMap(map, sets)
  if (!set) return 'install'
  const order = compareVersions(set.version, map.latest_tag)
  if (order !== null && order < 0) return 'update'
  return 'reinstall'
}
