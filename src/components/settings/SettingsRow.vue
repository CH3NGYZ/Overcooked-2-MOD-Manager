<script setup lang="ts">
import { ChevronRight } from 'lucide-vue-next'
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
    <span v-if="icon" class="ico"><component :is="icon" :size="19" :stroke-width="1.9" /></span>
    <span class="txt">
      <b>{{ title }}</b>
      <small v-if="description">{{ description }}</small>
    </span>
    <span class="right">
      <slot name="right" />
      <ChevronRight v-if="clickable" :size="18" class="chev" />
    </span>
  </div>
</template>

<style scoped>
.row {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 13px 16px;
  border: 1px solid #262c35;
  border-radius: 12px;
  background: #1b1e24;
  transition: background-color .16s ease, border-color .16s ease;
}
.row.clickable {
  cursor: pointer;
}
.row.clickable:hover {
  background: #21262e;
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
  color: #aeb9c6;
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
  color: #8d9bab;
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
  color: #617081;
}
</style>
