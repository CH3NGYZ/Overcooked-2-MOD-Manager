<script setup lang="ts" generic="T">
import { computed } from 'vue'
import { NCollapse, NCollapseItem, NButton, NTag, NAlert } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import SourceTitle from './SourceTitle.vue'
import SourceGroupSlot from './SourceGroupSlot.vue'
import { backendMessageOf } from '../locale/backend'
import { t } from '../i18n'
import type { SourceGroupSource } from '../composables/useSourceGroupTree'

const props = defineProps<{
  nodes: SourceGroupSource<T>[]
  /** 递归层通过稳定 getter 订阅根树，避免折叠插槽缓存旧数组。 */
  getTree?: () => SourceGroupSource<T>[]
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
const currentNodes = computed(() => props.getTree?.() ?? props.nodes)
const getTree = () => currentNodes.value
const children = computed(() => {
  const known = new Set(currentNodes.value.map(node => node.id))
  return currentNodes.value.filter(node => props.parentId
    ? node.parentId === props.parentId
    : !node.parentId || !known.has(node.parentId))
})
</script>

<template>
  <n-collapse class="source-collapse" :expanded-names="expanded" @update:expanded-names="emit('update:expanded', $event)">
    <n-collapse-item v-for="entry in children" :key="entry.id" :name="entry.id">
      <template #header>
        <SourceGroupSlot :get-tree="getTree" :source-id="entry.id" v-slot="{ node }">
          <div class="source-head" :data-source-id="node.id">
            <SourceTitle :label="node.label" :note-i18n="node.note_i18n" />
            <n-tag size="tiny" :bordered="false">{{ node.count }}</n-tag>
            <n-button size="tiny" quaternary :loading="node.status === 'pending' || node.status === 'loading' || busy(node)"
              :disabled="disabled" @click.stop="emit('refresh', node)">
              <template #icon><RefreshOutline width="13" height="13" /></template>
            </n-button>
          </div>
        </SourceGroupSlot>
      </template>
      <SourceGroupSlot :get-tree="getTree" :source-id="entry.id" v-slot="{ node }">
        <n-alert v-if="node.error" type="error" :show-icon="true" class="source-error">{{ backendMessageOf(node.error) }}</n-alert>
        <p v-else-if="node.status === 'ready' && node.count === 0 && !currentNodes.some(child => child.parentId === node.id)"
          class="source-empty">{{ t('source.noVisibleRepositories') }}</p>
        <slot v-else :node="node" />
      </SourceGroupSlot>
      <SourceDirectory :nodes="currentNodes" :get-tree="getTree" :expanded="expanded" :parent-id="entry.id" :disabled="disabled" :busy="busy"
        @refresh="emit('refresh', $event)" @update:expanded="emit('update:expanded', $event)">
        <template #default="scope"><slot v-bind="scope" /></template>
      </SourceDirectory>
    </n-collapse-item>
  </n-collapse>
</template>

<style scoped>
.source-collapse { width: 100%; }
.source-collapse .source-collapse { margin: -4px 0 0 14px; width: calc(100% - 14px); }
.source-collapse .source-collapse:empty { display: none; }
.source-head { display: flex; align-items: center; gap: 8px; min-width: 0; }
.source-error { margin: 10px 0; }
.source-empty { margin: 8px 0; color: var(--oc2-text-3); font-size: 13px; }
</style>
