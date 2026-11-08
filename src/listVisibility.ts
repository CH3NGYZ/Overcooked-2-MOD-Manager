/**
 * 本地管理页「列表可见性」的纯规则（不依赖 Vue / i18n）。
 *
 * 搜索词与状态标签是两个独立维度，叠加时取交集。两页的页头标签、计数、
 * 空态、折叠与批量开关都读这一份结论，避免一页筛选、另一页还在滚动定位。
 * `composables/useListVisibility.ts` 只负责把这些规则接上响应式状态。
 */
/** 单选：点另一个状态直接换过去，再点同一个取消，回到全览。 */
export function nextStatusKey(current: string | null, key: string): string | null {
  return current === key ? null : key
}

export type BulkSwitchValue = 'enable' | 'disable' | ''

/**
 * 将完整列表的启用计数投影为页头受控分段开关的选中值。
 * 空列表和部分启用都返回空串，保持控件受控且不伪造全开/全关状态。
 */
export function bulkSwitchValue(enabledCount: number, total: number): BulkSwitchValue {
  if (total <= 0) return ''
  if (enabledCount === total) return 'enable'
  if (enabledCount === 0) return 'disable'
  return ''
}

/** 没有选中状态时所有单元可见。 */
export function statusKeyMatches(active: string | null, keys: readonly string[]): boolean {
  return active === null || keys.includes(active)
}

/** 「命中 / 全部」只在筛选时出现，不筛选时不占页头位置。 */
export function visibilityCountLabel(
  filtering: boolean,
  visible: number,
  total: number,
): string | null {
  return filtering ? `${visible} / ${total}` : null
}

export interface VisibilityTexts {
  noData(): string
  noQueryMatch(): string
  noStatusMatch(): string
}

/**
 * 三种「空」要分开说：没装东西是数据问题，搜不到是输入没命中，当前状态
 * 筛不到是筛选条件太窄。搜索优先于状态，因为用户最先看到的是搜索框。
 */
export function visibilityEmptyText(
  options: {
    hasItems: boolean
    visible: number
    searching: boolean
    statusFiltering: boolean
  },
  texts: VisibilityTexts,
): string {
  if (!options.hasItems) return texts.noData()
  if (options.visible !== 0) return texts.noData()
  if (options.searching) return texts.noQueryMatch()
  if (options.statusFiltering) return texts.noStatusMatch()
  return texts.noData()
}
