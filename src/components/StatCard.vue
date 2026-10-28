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
    <div class="stat-value-row">
      <div class="stat-value" :class="tone ? tone : 'default'">{{ value }}</div>
      <!--
        次要计数（可更新 / 刷新失败）：与主数值并排，各自用自己的等级色。
        不并进主数值里，否则「4 个 MOD」和「1 个可更新」会被读成同一个数。
      -->
      <div v-if="$slots.extra" class="stat-extra">
        <slot name="extra" />
      </div>
    </div>
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
/*
 * 动作区：可容纳「提示文字 + 按钮」并排。
 * 提示紧贴按钮左侧，所以在同一行内用 flex 而不是让它们各自定位。
 */
.stat-action {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.stat-value {
  font-size: 30px;
  font-weight: 700;
  line-height: 1.1;
}
.stat-value-row {
  display: flex;
  align-items: baseline;
  gap: 10px;
  margin: 10px 0 6px;
}
/*
 * 次要计数与主数值同一行、基线对齐：`4 · 2 可更新`。
 * 用 baseline 而不是 center —— 主数值 30px、次数值 22px，
 * center 会让两者视觉重心错位，看起来像没对齐。
 */
.stat-extra {
  display: flex;
  align-items: baseline;
  gap: 4px;
}
/*
 * 颜色由使用方通过 `--stat-extra-color` 传入（或直接在 slot 里写 :style）。
 *
 * 为什么不用这里写死的 tone 变量：slot 内容由父组件渲染，
 * 父组件的 scoped class 命中不到它，所以颜色必须走内联样式或继承变量。
 *
 * 字号与主数值**完全相同**（30px）：`4 · 2 可更新` 读起来是一个整体，
 * 次要计数不应缩成小字——它在视觉上是同一句话的后半段。
 */
.extra-sep {
  color: var(--oc2-text-3);
  font-size: 30px;
  font-weight: 400;
  line-height: 1.1;
}
.extra-value {
  font-size: 30px;
  font-weight: 700;
  line-height: 1.1;
  color: var(--stat-extra-color, inherit);
}
.extra-label {
  font-size: 30px;
  font-weight: 400;
  line-height: 1.1;
  color: var(--stat-extra-color, inherit);
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
