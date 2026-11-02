import { reactive } from 'vue'

/**
 * 卡片开关的跨页状态快照。
 *
 * ## 为什么需要它
 *
 * 批量启停（「全部开启 / 全部关闭」）之后，页面上每个开关都要像用户手点一样
 * 走一遍 Naive `n-switch` 自带的值变化过渡。但列表里的卡片长在
 * `n-collapse-item` 的内容区里，那块内容由 naive-ui 的 `CollapseItemContent`
 * 拿稳定插槽渲染内部的 `FadeInExpandTransition`：那个组件 vnode 的 patchFlag
 * 是 0，父级重渲染时 Vue 的 `shouldUpdateComponent` 直接跳过它，插槽闭包不会
 * 重新求值 —— 卡片拿不到新的 `enabled`，开关停在批量前的状态（实测 25 个开关
 * 只有 21 个更新）。早先的修法是「换整棵列表的 key 强制重建」，代价是开关被
 * 重建：新节点一挂载就是目标状态，没有值变化过程，动画因此整个消失。
 *
 * ## 怎么绕开
 *
 * 插槽闭包只在**折叠过渡组件自己的 render effect** 里被调用（它 render 时才
 * `this.$slots.default?.()`），所以只要插槽体里读到的某个响应式值变了，折叠
 * 内容就会重渲染，并顺带拿到页面最新的插槽。这里就是那个响应式值：页面在
 * 插槽里按稳定键读开关值，批量成功后一次性发布新值 —— 折叠内容整片重建，
 * 卡片以同一个 DOM 节点收到 `value` 变化，过渡由 Naive 自己播。
 *
 * ## 快照不是真源
 *
 * 真源始终是后端扫描结果（页面的 `mods` / `sets`）。快照只覆盖「批量操作的
 * 结果比卡片 props 先到一步」这段窗口；每次数据重读都要整体重新发布，否则
 * 单卡开关之后快照会把开关按回旧值。
 */

/** 快照条目：稳定键 → 开关值。 */
export type CardSwitchEntry = readonly [key: string, value: boolean]

const values = reactive(new Map<string, boolean>())

/**
 * 一次性发布一组开关值（页面每次拿到新扫描结果后调用）。
 *
 * 只覆盖传进来的键：MOD 管理与地图管理各自是一个命名空间，
 * 不能因为这一页重读就把另一页的快照清空。
 */
export function publishCardSwitches(entries: Iterable<CardSwitchEntry>): void {
  for (const [key, value] of entries) values.set(key, value)
}

/** 读某个稳定键的已发布值；没发布过返回 `undefined`（调用方回退自己的实时值）。 */
export function cardSwitchValue(key: string): boolean | undefined {
  return values.get(key)
}

/** 读开关值：快照里有就用快照，否则回退调用方手里的实时值。 */
export function cardSwitchOn(key: string | undefined, fallback: boolean): boolean {
  if (!key) return fallback
  return values.get(key) ?? fallback
}

/** 清空快照（测试与「整页数据作废」时用）。 */
export function clearCardSwitches(): void {
  values.clear()
}

/**
 * MOD 的稳定键。
 *
 * `dll_file` 会随开关在 `X.dll` / `X.dll.disabled` 之间变，直接拿它当键的话
 * 每切一次就是一个新键，快照永远追不上卡片。
 */
export function modSwitchKey(dllFile: string): string {
  return `mod:${dllFile.replace(/\.disabled$/i, '')}`
}

/** 关卡集的稳定键：开关只把集目录搬进/搬出停用目录，目录名不变。 */
export function levelSetSwitchKey(dirName: string): string {
  return `level:${dirName}`
}

/** 关卡集随包插件的稳定键：不同集里允许出现同名 DLL。 */
export function levelSetPluginSwitchKey(dirName: string, fileName: string): string {
  return `level-plugin:${dirName}\u0000${fileName}`
}
