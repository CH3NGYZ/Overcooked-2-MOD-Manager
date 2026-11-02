<script setup lang="ts">
import { computed, inject } from 'vue'
import { LEVEL_SET_GROUP_CTX } from './levelSetGroupContext'

/**
 * 地图管理页的分组标题。
 *
 * `#header` 与内容槽在同一棵被跳过的子树里，所以标题也必须由组件自己渲染：
 * 组名随语言/绑定地图变化，写在页面插槽里切语言时会停在旧文案上。
 */
const props = defineProps<{ groupKey: string }>()

const ctx = inject(LEVEL_SET_GROUP_CTX)!

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
