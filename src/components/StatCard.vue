<script setup lang="ts">
import { NCard } from 'naive-ui'

defineProps<{
  label: string
  value: string
  sub?: string
  tone?: 'default' | 'success' | 'warning' | 'danger'
  /** 整张卡可点（打开详情/跳转）。内部控件需要 @click.stop 阻止冒泡。 */
  clickable?: boolean
}>()
defineEmits<{ (event: 'click'): void }>()
</script>

<template>
  <!--
    卡片 = 标签 + 主数值 + 说明 + 可选的右侧动作。
    动作放在标题行右侧而不是底部：用户扫视时先看「是什么状态」，
    再决定要不要动手，动作贴着标签更容易对应。
  -->
  <n-card class="stat-card" :class="{ clickable }" :bordered="true"
    @click="clickable && $emit('click')">
    <div class="stat-head">
      <div class="stat-label">{{ label }}</div>
      <div v-if="$slots.action" class="stat-action">
        <slot name="action" />
      </div>
    </div>
    <div class="stat-value" :class="tone ? tone : 'default'">{{ value }}</div>
    <div v-if="sub" class="muted stat-sub">{{ sub }}</div>
  </n-card>
</template>

<style scoped>
.stat-card {
  min-height: 128px;
}
/* 可点击卡片的视觉反馈：颜色 + 指针，与设置页可点行一致 */
.stat-card.clickable {
  cursor: pointer;
  transition: border-color .2s ease, background-color .2s ease;
}
.stat-card.clickable:hover {
  border-color: var(--oc2-accent);
}
.stat-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  min-width: 0;
}
.stat-label {
  color: var(--oc2-text-3);
  font-size: 12.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.stat-action { flex: 0 0 auto; }
.stat-value {
  margin: 10px 0 6px;
  font-size: 30px;
  font-weight: 700;
  line-height: 1.1;
}
.stat-value.success { color: var(--oc2-accent); }
.stat-value.warning { color: var(--oc2-accent); }
.stat-value.danger { color: #ee8585; }
/*
 * 说明文字最多两行：路径和仓库名可能很长，
 * 不限制会把卡片撑高，四张卡高度不一致。
 */
.stat-sub {
  display: -webkit-box;
  -webkit-line-clamp: 2;
  line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  font-size: 12.5px;
  line-height: 1.5;
}
</style>
