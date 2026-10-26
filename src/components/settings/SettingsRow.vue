<script setup lang="ts">
import { ChevronForwardOutline } from '@vicons/ionicons5'
import type { Component } from 'vue'

/**
 * Astral 式设置行条目：左侧图标 + 标题/描述，右侧插槽放按钮、开关或箭头。
 * clickable 时整行可点并显示右箭头（打开二级菜单的入口）。
 */
const props = defineProps<{
  icon?: Component
  title: string
  description?: string
  clickable?: boolean
}>()

const emit = defineEmits<{ click: [] }>()

function onClick() {
  if (props.clickable) emit('click')
}
</script>

<template>
  <div class="row" :class="{ clickable: clickable }" @click="onClick">
    <span v-if="icon" class="ico"><component :is="icon" width="19" height="19" /></span>
    <span class="txt">
      <b>{{ title }}</b>
      <small v-if="description">{{ description }}</small>
    </span>
    <span class="right">
      <slot name="right" />
      <ChevronForwardOutline v-if="clickable" width="18" height="18" class="chev" />
    </span>
  </div>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 13px 16px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-row);
  transition: background-color .16s ease, border-color .16s ease;
}
.row.clickable {
  cursor: pointer;
}
.row.clickable:hover {
  background: var(--oc2-row-hover);
  border-color: #34404d;
}
.ico {
  flex: 0 0 auto;
  width: 36px;
  height: 36px;
  display: grid;
  place-items: center;
  border-radius: 9px;
  background: #232830;
  color: var(--oc2-text-2);
}
.txt {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.txt b {
  font-size: 13.5px;
  font-weight: 600;
}
.txt small {
  color: var(--oc2-text-3);
  font-size: 11.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.right {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 8px;
}
.chev {
  color: var(--oc2-text-4);
}
</style>
