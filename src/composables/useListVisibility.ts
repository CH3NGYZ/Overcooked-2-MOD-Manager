/**
 * MOD 管理 / 地图管理共用的「列表可见性」状态机。
 *
 * 规则全部在 `listVisibility.ts`（纯函数，Node 直接测）；这里只把它们接上
 * 响应式状态。页面负责回答「我这个展示单元属于哪些状态」，选中态、切换、
 * 计数、空态、折叠与批量开关的反应两页完全一致。
 */
import { computed, ref, watch, type ComputedRef, type Ref } from 'vue'
import {
  nextStatusKey,
  statusKeyMatches,
  visibilityCountLabel,
  visibilityEmptyText,
  type VisibilityTexts,
} from '../listVisibility.ts'

export function useListVisibility(
  query: Ref<string> | ComputedRef<string>,
  texts: VisibilityTexts,
) {
  const activeStatus = ref<string | null>(null)
  const searching = computed(() => query.value.trim().length > 0)
  const filtering = computed(() => searching.value || activeStatus.value !== null)

  function reconcileStatuses(keys: ComputedRef<readonly string[]>) {
    watch(keys, available => {
      if (activeStatus.value && !available.includes(activeStatus.value)) activeStatus.value = null
    })
  }

  function toggleStatus(key: string) {
    activeStatus.value = nextStatusKey(activeStatus.value, key)
  }
  function isStatusActive(key: string) {
    return activeStatus.value === key
  }
  function statusMatches(keys: readonly string[]) {
    return statusKeyMatches(activeStatus.value, keys)
  }
  function countLabel(visible: number, total: number) {
    return visibilityCountLabel(filtering.value, visible, total)
  }
  function emptyText(hasItems: boolean, visible: number) {
    return visibilityEmptyText(
      {
        hasItems,
        visible,
        searching: searching.value,
        statusFiltering: activeStatus.value !== null,
      },
      texts,
    )
  }
  /** 筛选时强制全展开：命中项可能落在用户收起的分组里。 */
  const forceExpand = computed(() => filtering.value)
  /** 批量开关作用于全部项，筛选期间收起，避免误当成筛选范围。 */
  function showBulkSwitch(hasItems: boolean) {
    return hasItems && !filtering.value
  }
  /** 折叠事件在筛选期间不落库，清空后用户原本的折叠状态才不会丢。 */
  function ignoringCollapse() {
    return filtering.value
  }
  return {
    reconcileStatuses,
    activeStatus,
    searching,
    filtering,
    forceExpand,
    toggleStatus,
    isStatusActive,
    statusMatches,
    countLabel,
    emptyText,
    showBulkSwitch,
    ignoringCollapse,
  }
}
