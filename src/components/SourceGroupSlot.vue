<script setup lang="ts" generic="T">
import { computed } from 'vue'
import type { SourceGroupLeaf, SourceGroupSource } from '../composables/useSourceGroupTree'

/**
 * 折叠库缓存稳定插槽时，必须在独立渲染 effect 内重新按键取树节点。
 * 只接收 getter 和稳定身份，不捕获上一次过滤的 node/group 对象；不重建卡片。
 */
const props = defineProps<{
  getTree: () => SourceGroupSource<T>[]
  sourceId: string
  groupKey?: string
}>()
defineSlots<{ default(props: { node: SourceGroupSource<T>; group: SourceGroupLeaf<T> }): unknown }>()
const node = computed(() => props.getTree().find(item => item.id === props.sourceId))
const group = computed(() => node.value?.groups.find(item => item.key === props.groupKey))
const emptyGroup: SourceGroupLeaf<T> = { key: '', label: '', items: [] }
</script>
<template>
  <slot v-if="node && (!groupKey || group)" :node="node" :group="group ?? emptyGroup" />
</template>
