<script setup lang="ts">
import { computed, inject } from 'vue'
import { MOD_GROUP_CTX } from './levelSetGroupContext'

/**
 * MOD 管理页的分类标题。
 *
 * 与地图管理页同理：`#header` 也在被跳过的折叠子树里，分类名（语言切换后
 * 会变）必须由组件自己读上下文才会更新。
 */
const props = defineProps<{ groupKey: string }>()

const ctx = inject(MOD_GROUP_CTX)!

const label = computed(
  () => ctx.groups.find((group) => group.key === props.groupKey)?.label ?? '',
)
</script>

<template>
  <div class="group-head">
    <b>{{ label }}</b>
  </div>
</template>

<style scoped>
.group-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
.group-head b {
  font-size: 14px;
  color: var(--oc2-accent);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
