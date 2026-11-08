<script setup lang="ts" generic="T extends { provider: Provider; username: string; repository: string }">
import { computed } from 'vue'
import type { Provider } from '../types'
import { NCollapse, NCollapseItem, NButton, NTag, NAlert } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import SourceTitle from './SourceTitle.vue'
import SourceGroupSlot from './SourceGroupSlot.vue'
import { backendMessageOf } from '../locale/backend'
import type { SourceGroupSource } from '../composables/useSourceGroupTree'

const props = defineProps<{
  nodes: SourceGroupSource<T>[]
  expanded: string[]
  parentId?: string
  disabled?: boolean
  busy: (node: SourceGroupSource<T>) => boolean
}>()
const emit = defineEmits<{
  refresh: [node: SourceGroupSource<T>]
  'update:expanded': [names: string[]]
}>()
defineSlots<{ default(props: { node: SourceGroupSource<T> }): unknown }>()
const children = computed(() => {
  const known = new Set(props.nodes.map(node => node.id))
  return props.nodes.filter(node => props.parentId
    ? node.parentId === props.parentId
    : !node.parentId || !known.has(node.parentId))
})
</script>

<template>
  <n-collapse class="source-collapse" :expanded-names="expanded" @update:expanded-names="emit('update:expanded', $event)">
    <n-collapse-item v-for="entry in children" :key="entry.id" :name="entry.id">
      <template #header>
        <SourceGroupSlot :get-tree="() => nodes" :source-id="entry.id" v-slot="{ node }">
          <div class="source-head" :data-source-id="node.id">
            <SourceTitle :label="node.label" :source-url="node.url" :items="node.groups.flatMap(group => group.items)" />
            <n-tag size="tiny" :bordered="false">{{ node.count }}</n-tag>
            <n-button size="tiny" quaternary :loading="node.status === 'pending' || node.status === 'loading' || busy(node)"
              :disabled="disabled" @click.stop="emit('refresh', node)">
              <template #icon><RefreshOutline width="13" height="13" /></template>
            </n-button>
          </div>
        </SourceGroupSlot>
      </template>
      <SourceGroupSlot :get-tree="() => nodes" :source-id="entry.id" v-slot="{ node }">
        <n-alert v-if="node.error" type="error" :show-icon="true" class="source-error">{{ backendMessageOf(node.error) }}</n-alert>
        <slot v-else :node="node" />
      </SourceGroupSlot>
      <SourceDirectory :nodes="nodes" :expanded="expanded" :parent-id="entry.id" :disabled="disabled" :busy="busy"
        @refresh="emit('refresh', $event)" @update:expanded="emit('update:expanded', $event)">
        <template #default="scope"><slot v-bind="scope" /></template>
      </SourceDirectory>
    </n-collapse-item>
  </n-collapse>
</template>

<style scoped>
.source-collapse { width: 100%; }
.source-collapse .source-collapse { margin: 12px 0 0 12px; width: calc(100% - 12px); }
.source-head { display: flex; align-items: center; gap: 8px; min-width: 0; }
.source-error { margin: 10px 0; }
</style>
