<script setup lang="ts">
import { computed } from 'vue'
import { NCard } from 'naive-ui'
import { tagColorFor } from '../theme'

/**
 * 总览页统计卡的唯一实现。
 *
 * 每张卡只回答「现在要不要管」：主数值是数量或状态，副标题补充口径，
 * 次要计数说明「其中多少需要处理」，右侧动作是跳转入口。框架卡与
 * 四张业务卡（在线/本地 × MOD/地图）都走这一个组件，页面只传数据 ——
 * 标题行、数值行、计数排版的规则因此只维护一份。
 */
defineProps<{
  label: string
  value: string
  sub?: string
  tone?: 'default' | 'success' | 'warning' | 'danger'
  /**
   * 次要计数（`26 · 1 刷新失败` / `4 · 2 可更新`）。
   *
   * `count` 为 0 时组件自己隐藏：四张卡的隐藏规则完全一致，
   * 留在组件里比让每个调用方各写一次 `v-if` 更不容易漏。
   */
  extra?: { count: number; label: string }
  /** 整张卡可点（打开详情/跳转）。内部控件需要 @click.stop 阻止冒泡。 */
  clickable?: boolean
}>()
defineEmits<{ (event: 'click'): void }>()

/**
 * 次要计数的文字色 = 同卡按钮的填充色。
 *
 * `tagColorFor('moderate').textColor` 与 Naive `type="warning"` 按钮的
 * `warningColor` 是同一个值（都是主题对比色），所以文字与按钮永远同色，
 * 用户换主色时一起变。计数由本组件渲染，颜色因此只在这里定一次。
 */
const extraColor = computed(() => tagColorFor('moderate').textColor)
</script>

<template>
  <!--
    卡片 = 标签 + 主数值 + 计数 + 说明 + 可选动作。
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
      <div v-if="extra && extra.count > 0" class="stat-extra"
        :style="{ '--stat-extra-color': extraColor }">
        <span class="extra-sep">·</span>
        <span class="extra-value">{{ extra.count }}</span>
        <span class="extra-label">{{ extra.label }}</span>
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
 * baseline 让分隔符、数值、标签落在同一条基线上，读起来是一个整体。
 */
.stat-extra {
  display: flex;
  align-items: baseline;
  gap: 4px;
}
/*
 * 颜色由 `--stat-extra-color` 传入（见 extraColor）：
 * 数值与标签同色，分隔符保持中性灰。
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
