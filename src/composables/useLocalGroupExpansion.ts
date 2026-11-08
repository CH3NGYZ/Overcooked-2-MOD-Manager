import { computed, ref, type ComputedRef, type Ref } from 'vue'
import { collapsedGroupNames, expandedGroupNames, expandGroupName } from '../groupExpansion.ts'

export function useLocalGroupExpansion(
  allKeys: Ref<readonly string[]> | ComputedRef<readonly string[]>,
  forceExpand: Ref<boolean> | ComputedRef<boolean>,
  ignoringCollapse: () => boolean,
) {
  const collapsedKeys = ref<string[]>([])
  const expandedKeys = computed(() => expandedGroupNames(
    [...allKeys.value],
    collapsedKeys.value,
    forceExpand.value,
  ))

  function onExpanded(names: string[]) {
    if (ignoringCollapse()) return
    collapsedKeys.value = collapsedGroupNames([...allKeys.value], names)
  }

  function isCollapsed(key: string) {
    return collapsedKeys.value.includes(key)
  }

  function expandGroup(key: string) {
    collapsedKeys.value = expandGroupName(collapsedKeys.value, key)
  }

  return { collapsedKeys, expandedKeys, onExpanded, isCollapsed, expandGroup }
}
