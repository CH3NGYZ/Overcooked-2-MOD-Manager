import type { ComputedRef, CSSProperties, InjectionKey, Ref, UnwrapNestedRefs } from 'vue'
import type { LevelSetAuthorNames, LevelSetCardAction, LevelSetCardTag } from '../levelSetStatus'
import type { DependencyRow } from './DependencyTable.vue'
import type { StatusActionSpec } from '../statusActions'
import type { PackageStatus } from '../statusTags'
import type { InstalledMod, LevelSetInfo, LevelSetPlugin, MapInfo } from '../types'

/**
 * 分组内容组件的上下文。
 *
 * ## 为什么卡片要放到独立组件里
 *
 * 两个管理页的分组是 `n-collapse` + `n-collapse-item`，组内卡片写在内容槽里。
 * `n-collapse` 的 vnode 编译出来 `patchFlag = 8`（只比动态 props），而动态
 * prop 只有 `expanded-names`：同组删掉一张卡时组集合没变 → `expandedNames`
 * 拿到同一个数组引用 → Vue 判定整棵子树无需更新，插槽闭包不重跑，组内
 * `v-for` 停在旧数组上（删空一个组才会因为组键变化而更新）。
 *
 * 所以组内的 `v-for` 不能留在页面插槽里，必须搬进组件：组件实例有自己的
 * 渲染 effect，父级被跳过也照样按上下文数据重渲。上下文因此必须**存响应式
 * 引用**（ref/computed）而不是快照值 —— 页面用 `reactive()` 包一层 provide，
 * 组件模板里读 `ctx.groups` 才跟得到更新。
 */

/** 卡片工具栏的按钮宽度（两页共用同一组数字，避免 JS 算列宽与 CSS 排间距漂移）。 */
export const TOOL_BUTTON_WIDTH = 30
/** 相邻工具栏按钮之间的空档（两端不加）。 */
export const TOOL_GAP = 8

/** 地图管理页的分组节点。 */
export interface LevelSetGroupNode {
  key: string
  label: string
  sets: LevelSetInfo[]
}

/** MOD 管理页的包节点（一个 DLL 一张卡，或 `package_id` 相同的多张合成一个外壳）。 */
export interface ModPackageNode {
  key: string
  label: string
  mods: InstalledMod[]
  /** 搜索命中的展示成员；包级开关、卸载、依赖和状态仍使用完整 mods。 */
  visibleMods: InstalledMod[]
  grouped: boolean
  enabled: boolean
  partial: boolean
  statuses: PackageStatus[]
  action?: StatusActionSpec
  repositoryId?: string
}

/** MOD 管理页的分组节点。 */
export interface ModGroupNode {
  key: string
  label: string
  packages: ModPackageNode[]
}

/**
 * 卡片展开状态里内容槽组件真正要用的两个判定。
 *
 * 结构化类型：`useCardExpansion` 的返回值直接满足，页面不需要再包一层。
 */
export interface GroupExpansion {
  isOuterOpen(key: string): boolean
  isInnerOpen(parent: string, child: string): boolean
}

/** `LevelsView` 提供给组内容组件的上下文。 */
export interface LevelSetGroupContext {
  groups: ComputedRef<LevelSetGroupNode[]>
  busy: Ref<Record<string, boolean>>
  deleting: Ref<Record<string, boolean>>
  expansion: GroupExpansion
  highlightId: Ref<string | null>
  toolsMinWidth: ComputedRef<number>
  kitchenRunning: ComputedRef<boolean>
  toggle(set: LevelSetInfo, enabled: boolean): void | Promise<void>
  activate(set: LevelSetInfo, card?: HTMLElement): void | Promise<void>
  tagsOf(set: LevelSetInfo): LevelSetCardTag[]
  tagTitle(set: LevelSetInfo, tag: LevelSetCardTag): string | undefined
  onTagClick(tag: LevelSetCardTag): void
  /** 卡片状态区的一键动作（可更新 / 缺依赖 / 依赖版本冲突）。 */
  actionOf(set: LevelSetInfo): LevelSetCardAction | undefined
  /** 这一个动作正在执行（按钮转圈）。 */
  actionBusy(set: LevelSetInfo): boolean
  /** 别的导入 / 更新占着安装链路，按钮禁用。 */
  actionDisabled(set: LevelSetInfo): boolean
  runCardAction(set: LevelSetInfo): void | Promise<void>
  authorNamesOf(set: LevelSetInfo): LevelSetAuthorNames
  bindingOf(set: LevelSetInfo): { map: MapInfo; name: string; tag: string } | undefined
  hintOf(set: LevelSetInfo): string
  focusIdOf(set: LevelSetInfo): string | undefined
  removeSet(set: LevelSetInfo): void | Promise<void>
  openDir(set: LevelSetInfo): void | Promise<void>
  pluginTitle(plugin: LevelSetPlugin): string
  togglePlugin(
    set: LevelSetInfo,
    plugin: LevelSetPlugin,
    card?: HTMLElement,
  ): void | Promise<void>
  dependencyRows(set: LevelSetInfo): DependencyRow[]
  openDependencyJump(row: DependencyRow): void
  linkToMap(mapId?: string): void
}

/** `ModsLocalView` 提供给组内容组件的上下文。 */
export interface ModGroupContext {
  groups: ComputedRef<ModGroupNode[]>
  busy: Ref<Record<string, boolean>>
  expansion: GroupExpansion
  highlightId: Ref<string | null>
  toolsMinWidth: ComputedRef<number>
  toolGroupStyle: ComputedRef<CSSProperties>
  kitchenRunning: ComputedRef<boolean>
  actionByDll: ComputedRef<Record<string, StatusActionSpec>>
  previewing: Ref<Record<string, boolean>>
  refreshing: Ref<Record<string, boolean>>
  modCardKey(dllFile: string): string
  modAnchor(mod: InstalledMod): string
  bindingRepositoryId(mod: InstalledMod): string | undefined
  singleModTitle(mod: InstalledMod): string
  modTitle(mod: InstalledMod): string
  isStatusPulsing(mod: InstalledMod): boolean
  toggle(mod: InstalledMod, enabled: boolean): void | Promise<void>
  onCardClick(event: MouseEvent, mod: InstalledMod, packageKey?: string): void
  runCardAction(mod: InstalledMod): void | Promise<void>
  runPackageAction(pkg: ModPackageNode): void | Promise<void>
  openLogs(mod: InstalledMod): void | Promise<void>
  editConfig(mod: InstalledMod): void
  openLocation(mod: InstalledMod): void | Promise<void>
  remove(mod: InstalledMod): void | Promise<void>
  bindingText(mod: InstalledMod): string
  statusSummary(mod: InstalledMod): string
  packageCountText(mod: InstalledMod): string
  modDependencyRows(mod: InstalledMod): DependencyRow[]
  packageDependencyRows(pkg: ModPackageNode): DependencyRow[]
  openDependencyJump(row: DependencyRow): void
  togglePackage(pkg: ModPackageNode, event: MouseEvent): void
  togglePackageEnabled(pkg: ModPackageNode, enabled: boolean): void | Promise<void>
  removePackage(pkg: ModPackageNode): void | Promise<void>
  packageTagTargetId(pkg: ModPackageNode, key: string): string | undefined
  packageTagTitle(pkg: ModPackageNode, key: string): string | undefined
  openPackageTag(pkg: ModPackageNode, key: string): void
  linkToRepository(repositoryId: string): void
}

/** `reactive()` 会把 ref/computed 字段解包，模板里才拿得到裸值。 */
export type LevelSetGroupContextValue = UnwrapNestedRefs<LevelSetGroupContext>
/** 同上，MOD 管理页。 */
export type ModGroupContextValue = UnwrapNestedRefs<ModGroupContext>

export const LEVEL_SET_GROUP_CTX: InjectionKey<LevelSetGroupContextValue> =
  Symbol('level-set-group-ctx')
export const MOD_GROUP_CTX: InjectionKey<ModGroupContextValue> = Symbol('mod-group-ctx')
