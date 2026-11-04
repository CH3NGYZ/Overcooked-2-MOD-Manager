/**
 * 依赖表行的统一构造：MOD 安装页、MOD 管理页、地图下载页、地图管理页共用。
 *
 * ## 为什么要有这个模块
 *
 * 依赖表出现在四个地方，但「一行长什么样」在这四处的口径必须完全一致：
 * 同一个 GUID 在安装页叫「主机实用 MOD（街机 MOD）测试版」，在管理页却叫
 * `com.ch3ngyz.plugin.HostUtilities`，用户会以为是两个东西。所以目标名解析、
 * 状态文案、色调、跳转目标全部收在这里，页面只负责把数据递进来、把行递给
 * `DependencyTable`。
 *
 * ## 名称解析顺序（产品定过的口径）
 *
 * 1. **在线 MOD 卡片名**：仓库配置里作者写好的 `name_i18n`（按 GUID + 发布线
 *    定位，见 `relationTargetRepository`）——这是作者对这个 MOD 的正式称呼。
 * 2. **本地已装 MOD 的包标题**：安装那一刻从线上固化进清单的快照
 *    （`package_name_i18n`），仓库下线后仍能显示，而不是退化成 GUID。
 * 3. **GUID**：以上都没有时才是它。GUID 是事实，但不是名字。
 *
 * ## 为什么是纯模块
 *
 * 被 `tests/dependency-rows.test.mjs` 用 `node --test` 直接 import。引入
 * `vue`/`naive-ui`/`i18n` 都会让测试进程启动即崩，所以文案走 `translate`
 * 回调（视图传 `t`，测试传恒等函数），本地化名字走 `pickName` 回调。
 */
import {
  relationTargetRepository,
  relationsForMapRelease,
  resolveRelation,
  relationsForRelease,
  stricterRelation,
  type RelationStatus,
} from './modRelations.ts'
import { isPrereleaseVersion, repositoryForMod } from './modStatus.ts'
import { mapForLevelSet, mapReleaseVersion } from './levelSetStatus.ts'
import { formatVersionSpec } from './versionSpec.ts'
import type { TagTone } from './theme'
import type {
  InstalledMod, LevelSetInfo, MapInfo, RelationKind, Repository, RepositoryRelation,
} from './types'

/**
 * 状态标签的跳转目标。
 *
 * 只有「点一下就能去解决」的状态才带它：可安装 / 可更新 都意味着安装页里
 * 有一张能提供满足区间版本的仓库卡片，跳过去就是解法的入口；缺依赖（线上
 * 根本没有这个 GUID）与冲突没有可去的地方，保持纯文字。
 */
export interface DependencyJump {
  /** 目标仓库卡片的 `data-focus-id`（安装页据此高亮）。 */
  id: string
  /** 悬停说明（已翻译），说清点了会跳到哪。 */
  title: string
}

export interface DependencyRow {
  /** 稳定 key（GUID + 关系种类 + 发布线），用于 `v-for`。 */
  key: string
  /** 依赖目标名：仓库名、本地 MOD 包名，或解析不出名字时的 GUID。 */
  name: string
  /** 关系种类；省略时按 `requires` 处理。 */
  kind?: RelationKind
  /**
   * 关系限定的发布线（来自关系的 `prerelease` 三态）。
   *
   * `conflicts` 用它区分「排斥某个版本区间」与「和另一条发布线互斥」，
   * 表格里额外标出「测试版 / 正式版」。
   */
  line?: 'prerelease' | 'stable'
  /** 版本要求文本（区间表达式原文，如 `>=1.2.0,<2.0.0`）；空表示不限版本。 */
  requirement?: string
  /** 状态文案（已满足 / 可安装 / 可更新 / 缺依赖 / 冲突 / 未安装 …）。 */
  status: string
  /** 状态色调，走 `theme.ts` 的统一刻度。 */
  tone?: TagTone
  /** 有解法可去时给出目标，标签变成可点；省略表示这行无处可去。 */
  jump?: DependencyJump
}

/**
 * 翻译函数（`t`）；参数与 vue-i18n 的命名插值同形。
 *
 * 参数类型对齐 `i18n.ts` 的 `t`（`string | number` 值）：这里放宽成
 * `unknown` 会让每个调用点都要包一层无意义的转换。
 */
export type DependencyTranslate = (key: string, params?: Record<string, string | number>) => string

/**
 * 本地化名字取值函数。
 *
 * 默认实现够测试用，但**视图必须传 `pickI18n`**：它才是全站「当前语言 →
 * zh-CN → 任意非空」的唯一口径，在依赖表里另写一份回退顺序迟早会和其他
 * 地方不一致。
 */
export type DependencyPickName = (
  map: Record<string, string> | undefined,
  fallback: string,
) => string

/**
 * 表格的展示口径。
 *
 * 只有一套：依赖由「声明了同一 GUID 的在线仓库」满足，未满足时区分
 * 「可安装 / 可更新 / 缺依赖」，能装的行可以跳过去解决。
 *
 * 地图曾经走另一套（「未安装 / 版本不符」且不可跳转），理由是「地图的依赖是
 * 框架、安装入口在设置页」。这条理由不成立：地图声明的前置就是仓库里的 MOD
 * （DIYLevel、Web Runtime 都有 `plugin_guid`），能装就该能给入口。
 */
export interface DependencyRowOptions {
  /** 本地已装 MOD（判断满足情况的对象）。 */
  mods?: readonly InstalledMod[]
  /** 在线 MOD 仓库（取名与找安装来源）。 */
  repositories?: readonly Repository[]
  /** 声明方仓库：排除「装自己」，也是状态跳转的排除项。 */
  ownerRepository?: Repository
  /** 文案翻译，缺省原样返回 key（测试/纯计算场景）。 */
  translate?: DependencyTranslate
  /** 名字本地化，缺省 zh-CN → 任意非空 → 回退值。 */
  pickName?: DependencyPickName
  /**
   * 状态标签是否可跳转到提供该依赖的仓库卡片。
   *
   * 四类调用方（MOD 安装 / MOD 管理 / 地图下载 / 地图管理）口径一致：
   * 有可安装来源就跳，没有就不假装可点。
   */
  jumpToRepository?: boolean
  /**
   * 要判定的版本（地图域）：地图详情里选中哪个 Release，依赖表就看哪一版
   * 的 `release_relations`。省略时用「将要安装的最新版」，已安装的地图集
   * 传自己的本地版本。
   */
  version?: string | null
}

const identityTranslate: DependencyTranslate = (key) => key

const defaultPickName: DependencyPickName = (map, fallback) => {
  const chinese = map?.['zh-CN']?.trim()
  if (chinese) return chinese
  const any = Object.values(map ?? {}).find((value) => value.trim().length > 0)
  return any ?? fallback
}

/** GUID 比较忽略大小写与首尾空白（与 `modRelations` 同一条规则）。 */
function sameGuid(left: string, right: string): boolean {
  return left.trim().toLowerCase() === right.trim().toLowerCase()
}

/**
 * 版本要求文本：区间表达式按可读形式原样展示（`>=1.2.0,<2.0.0`）。
 *
 * 前端没有第二套「下限/上限」结构了，所以这里也不能把表达式再拆成
 * `≥ x` / `x ~ y` —— 那会把 `!=1.3`、`>1.0` 这类单条件表达不出来。
 */
export function dependencyRequirement(relation: RepositoryRelation): string {
  return formatVersionSpec(relation.version ?? '')
}

/** 关系的发布线小标：没写 `prerelease` 就是不限定（两条线都算）。 */
export function dependencyLine(relation: RepositoryRelation): DependencyRow['line'] {
  if (relation.prerelease === undefined) return undefined
  return relation.prerelease ? 'prerelease' : 'stable'
}

/** 本地装了某个 GUID 的所有 MOD（含被禁用的 —— 禁用不等于没装）。 */
function installedModsForGuid(mods: readonly InstalledMod[], guid: string): InstalledMod[] {
  return mods.filter((mod) => !!mod.guid && sameGuid(mod.guid, guid))
}

/**
 * 这条关系在本地对应哪一个 MOD。
 *
 * 关系可以限定发布线（正式版与测试版是同一 GUID 的两个包），所以先按
 * `prerelease` 三态过滤；不限定或过滤不到时退回第一个副本 —— 至少名字
 * 是对的，判定的满足性由 `resolveRelation` 独立负责。
 */
function localModForRelation(
  relation: RepositoryRelation,
  mods: readonly InstalledMod[],
): InstalledMod | undefined {
  const candidates = installedModsForGuid(mods, relation.guid)
  if (!candidates.length) return undefined
  const want = relation.prerelease
  if (want === undefined) return candidates[0]
  return candidates.find((mod) => isPrereleaseVersion(mod.version, mod.prerelease) === want)
    ?? candidates[0]
}

/**
 * 依赖目标的展示名：在线仓库名 → 本地安装包标题 → GUID。
 *
 * 单独导出是因为卡片上的「缺依赖」悬停提示也只需要名字，不需要整行。
 */
export function dependencyTargetName(
  relation: RepositoryRelation,
  options: DependencyRowOptions = {},
): string {
  const {
    repositories = [], mods = [], ownerRepository,
    pickName = defaultPickName,
  } = options

  const online = relationTargetRepository(relation, repositories, ownerRepository?.id)
  if (online) {
    const label = pickName(online.name_i18n, online.name).trim()
    if (label) return label
  }

  /*
   * 本地包标题是**安装时从线上固化**的快照，不是 DLL 里的插件名：
   * 作者在仓库里写的名字才是用户认得的那个，插件名常常是英文代号。
   */
  const local = localModForRelation(relation, mods)
  if (local) {
    const packageLabel = pickName(local.package_name_i18n, local.package_name ?? '').trim()
    if (packageLabel) return packageLabel
    const title = local.title?.trim()
    if (title) return title
    const name = local.name?.trim()
    if (name) return name
  }

  return relation.guid
}

/**
 * 一条关系的状态文案与色调。
 *
 * 冲突/兼容与依赖是两套语义：`conflicts` 的「没命中」是**好事**（无冲突），
 * 不能写成「已满足」—— 用户会以为是「我已经装好了」。
 */
function statusFor(
  status: RelationStatus,
  translate: DependencyTranslate,
): { text: string; tone: TagTone } {
  const kind = status.relation.kind ?? 'requires'
  const ok = status.state === 'ok'

  if (kind === 'conflicts') {
    return ok
      ? { text: translate('tools.noConflict'), tone: 'mild' }
      : { text: translate('modsOnline.conflict'), tone: 'critical' }
  }
  if (kind === 'compatible') {
    return ok
      ? { text: translate('tools.compatible'), tone: 'mild' }
      : { text: translate('tools.incompatible'), tone: 'critical' }
  }

  /*
   * 满足时只写「已满足」，**不写版本号**：版本号写在「要求」列里（那是
   * 声明的区间），状态列只回答「现在行不行」。把本地版本塞进状态会让同一
   * 行出现两个版本号，还得让用户自己比较哪个是要求、哪个是现状。
   */
  if (ok) return { text: translate('tools.satisfied'), tone: 'mild' }

  if (status.state !== 'installable') {
    return { text: translate('modsOnline.missingDep'), tone: 'critical' }
  }
  /*
   * 装了但版本不满足 →「可更新」，没装 →「可安装」。两者都要用户动手，
   * 但「更新已有的」和「装一个没有的」是两件事，混成一个词会让人以为
   * 本地根本没这个东西。
   */
  return status.installedMod
    ? { text: translate('tools.updatable'), tone: 'moderate' }
    : { text: translate('tools.installable'), tone: 'moderate' }
}

/** 一条关系的完整表格行（内部实现，导出便于单测单条）。 */
export function dependencyRowFromRelation(
  relation: RepositoryRelation,
  options: DependencyRowOptions = {},
): DependencyRow {
  const {
    mods = [], repositories = [], ownerRepository,
    translate = identityTranslate, jumpToRepository = false,
  } = options

  const status = resolveRelation(relation, mods, repositories, ownerRepository)
  const kind = relation.kind ?? 'requires'
  const line = dependencyLine(relation)
  const { text, tone } = statusFor(status, translate)

  const row: DependencyRow = {
    // key 必须带上发布线：同一 GUID 的「与测试版互斥 / 与正式版互斥」是两条
    // 不同声明，只用 guid:kind 会让 v-for 出现重复 key
    key: `${relation.guid}:${kind}:${line ?? 'any'}`,
    name: dependencyTargetName(relation, options),
    kind,
    line,
    requirement: dependencyRequirement(relation),
    status: text,
    tone,
  }

  /*
   * 可安装 / 可更新指向「能提供满足区间版本的仓库卡片」：唯一一处点一下
   * 就能把依赖补上的地方。缺依赖与冲突没有这种去处，保持纯文字。
   */
  if (jumpToRepository && status.state === 'installable' && status.repository) {
    row.jump = {
      id: status.repository.id,
      title: translate('modsOnline.viewDependencyInModInstall'),
    }
  }

  return row
}

/**
 * 一组关系声明 → 表格行。
 *
 * 可选依赖（`requires` + `optional`）不参与展示：它们缺失是正常状态，
 * 列出来只是噪音。与 `resolveRelations` 的过滤口径一致。
 */
export function dependencyRowsFor(
  relations: readonly RepositoryRelation[],
  options: DependencyRowOptions = {},
): DependencyRow[] {
  const rows: DependencyRow[] = []
  for (const relation of relations) {
    const kind = relation.kind ?? 'requires'
    if (kind === 'requires' && relation.optional) continue
    rows.push(dependencyRowFromRelation(relation, options))
  }
  return rows
}

/* ===== 卡片上的关系提醒标签 → 见 `modRelations.dependencyTagFor` ===== */

/**
 * 地图仓库声明的关系（`required_frameworks` 是它的旧投影，见下方回退）。
 *
 * `version` 是这次要安装的 Release 版本：覆盖表（`release_relations`）按它选；
 * 不传时用当前最新版（`mapReleaseVersion`）—— 卡片上「将要安装的版本」正是它。
 */
export function mapRelations(
  map: MapInfo | null | undefined,
  version?: string | null,
): RepositoryRelation[] {
  if (!map) return []
  return relationsForMapRelease(map, version ?? mapReleaseVersion(map))
}

/**
 * 一个已安装 MOD 该用哪份关系声明。
 *
 * **在线与本地不一致时以在线为准**（产品口径）：绑定仓库还在配置里，就用
 * 仓库当前的声明（并按已装版本筛 `release_relations`）；仓库不在了（源下线、
 * 自定义源被删、社区仓库被官方覆盖隐藏）才退回安装清单快照，否则依赖表会
 * 凭空消失。
 */
export function installedModRelations(
  mod: InstalledMod,
  repositories: readonly Repository[],
): RepositoryRelation[] {
  const repo = repositoryForMod([...repositories], mod)
  if (!repo) return mod.relations ?? []
  /*
   * 传已装版本：仓库可以按版本区间声明不同依赖（`release_relations`），
   * 拿最新版的关系去解释旧版本会给出错误结论。
   */
  return relationsForRelease(repo, mod.version ?? null)
}

/**
 * 计算关卡集依赖时真正需要的字段。
 *
 * ZIP 导入确认弹窗里的集还没落盘，只有 `info` 解析出来的身份与版本
 * （`ZipLevelSet`），没有 `uid`、`path`、`plugins` 这些已安装侧字段；而
 * 「绑定了哪张在线地图」只取决于身份（中文名 + 英文名 + 作者）与本地关系
 * 快照。把入参收窄到这个交集，确认界面与地图管理页才能共用同一套依赖判定，
 * 而不是给弹窗另写一份只看快照的简化版。
 */
export type LevelSetRelationSource =
  Pick<LevelSetInfo, 'name' | 'author' | 'version' | 'relations'> & { name_zh?: string | null }

/** 一个关卡集该用哪份关系声明（在线地图 → 本地快照）。 */
export function levelSetRelations(
  set: LevelSetRelationSource,
  maps: readonly MapInfo[],
): RepositoryRelation[] {
  const bound = mapForLevelSet(set, maps)
  // 已安装的集按**本地版本**取覆盖表：依赖表回答的是「这一版缺什么」，
  // 不是「最新版要不要什么」。本地版本读不出（空串）时退回默认声明。
  if (bound) return mapRelations(bound, set.version)
  return set.relations ?? []
}

/** 已安装 MOD 的依赖表行（MOD 管理页 / 详情用）。 */
export function installedModDependencyRows(
  mod: InstalledMod,
  options: Omit<DependencyRowOptions, 'ownerRepository'> = {},
): DependencyRow[] {
  const repositories = options.repositories ?? []
  return dependencyRowsFor(installedModRelations(mod, repositories), {
    ...options,
    ownerRepository: repositoryForMod([...repositories], mod),
    jumpToRepository: options.jumpToRepository ?? true,
  })
}

/**
 * 地图源条目的依赖表行（地图下载页详情用）。
 *
 * 与 MOD 侧同一套口径：地图声明的前置（DIYLevel、Web Runtime）本身就是仓库
 * 里的 MOD，能装就跳过去装，没有来源才是纯文字「缺依赖」。
 */
/**
 * 多 DLL 包外壳的依赖表行：把包内每个成员自己的关系表合起来看。
 *
 * ## 为什么不能只取一个代表成员
 *
 * 包的关系声明挂在仓库上，但 `release_relations` 按**已装版本**筛：一个包里
 * 装着 3.10.2 与 2.0.0 两支 DLL 时，前者命中 `==3.10.2` 覆盖规则（要求
 * DIYLevel ≥ 0.10.0），后者只有仓库级声明（不设区间）。只取首个成员当代表，
 * 依赖表就会随成员顺序变脸：表里写「已满足」，包标题却按另一支成员算出
 * 「缺依赖」—— 同一张卡上下自相矛盾，用户只能来问「低版本怎么还显示缺依赖」。
 *
 * 合并口径：按 `(kind, guid)` 去重，`requires` 取区间下限更高的一条
 * （`stricterRelation`），其余种类保留先出现的。包对外只该有一个结论。
 */
export function mergedModDependencyRows(
  members: readonly InstalledMod[],
  options: Omit<DependencyRowOptions, 'ownerRepository'> = {},
): DependencyRow[] {
  const repositories = options.repositories ?? []
  const merged: RepositoryRelation[] = []
  let ownerRepository: Repository | undefined
  for (const mod of members) {
    if (mod.is_dependency) continue
    ownerRepository ??= repositoryForMod([...repositories], mod)
    for (const relation of installedModRelations(mod, repositories)) {
      const kind = relation.kind ?? 'requires'
      const guid = relation.guid.trim().toLowerCase()
      const index = merged.findIndex((existing) => (
        (existing.kind ?? 'requires') === kind && existing.guid.trim().toLowerCase() === guid
      ))
      if (index < 0) merged.push(relation)
      else merged[index] = stricterRelation(merged[index], relation)
    }
  }
  if (!merged.length) return []
  return dependencyRowsFor(merged, {
    ...options,
    ownerRepository,
    jumpToRepository: options.jumpToRepository ?? true,
  })
}

export function mapDependencyRows(
  map: MapInfo | null | undefined,
  options: Omit<DependencyRowOptions, 'ownerRepository'> = {},
): DependencyRow[] {
  return dependencyRowsFor(mapRelations(map, options.version), {
    ...options,
    jumpToRepository: options.jumpToRepository ?? true,
  })
}

/** 本地关卡集的依赖表行（地图管理页详情用；与地图下载页同一口径）。 */
export function levelSetDependencyRows(
  set: LevelSetRelationSource,
  options: Omit<DependencyRowOptions, 'ownerRepository'> & {
    /**
     * 当前地图源列表（必填）。
     *
     * 它决定「这个集绑定了哪张在线地图」：在线条目在就以它的声明为准，
     * 不在才用本地快照。传漏会静默退化成只看快照，所以这里不给默认值。
     */
    maps: readonly MapInfo[]
  },
): DependencyRow[] {
  const { maps, ...rest } = options
  return dependencyRowsFor(levelSetRelations(set, maps), {
    ...rest,
    jumpToRepository: rest.jumpToRepository ?? true,
  })
}
