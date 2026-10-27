<script setup lang="ts">
/**
 * 统一的应用弹窗外壳。
 *
 * 解决的问题：Naive 的 n-modal + preset="card" 默认高度由内容决定，
 * 内容一长（展开详情、长提交日志）弹窗就会超出可视区域，底部按钮被顶到
 * 视口外，用户必须先滚整个页面才找得到；而窗口本身不可滚动时直接够不到。
 *
 * 这里固定三件事，所有弹窗共用：
 *   1. 弹窗尺寸永远不超过可视区域（宽最多 maxWidth，高最多 90% 可用高度）；
 *   2. 头部与底部 `#footer` 槽位是固定区，不参与滚动；
 *   3. 只有中间的默认插槽滚动，内容多长都不会撑开弹窗。
 *
 * 不带按钮的弹窗不传 `#footer` 即可，内容区自动独占剩余高度。
 *
 * 高度约束必须用 `:content-style` 内联下发：Naive 的 `.n-card-content`
 * 自带 `flex: 1` 但**没有** `min-height: 0`，而 flex item 的 min-height
 * 默认是 auto —— 只靠外部 CSS 类去改，内容会把 card 撑破（实测溢出到卡片外，
 * 还同时出现「弹窗不变高、内容被裁掉、明明溢出却滚不动」三个症状）。
 */
import { computed, toRefs } from 'vue'
import { NModal } from 'naive-ui'

const props = withDefaults(defineProps<{
  show: boolean
  /** 卡片标题；不传则不渲染头部。 */
  title?: string
  /** 最大宽度，默认 80vw 与 1180px 取小。 */
  maxWidth?: string
  /** 最大高度，默认可用视口的 90%。 */
  maxHeight?: string
  bordered?: boolean
  maskClosable?: boolean
  /** 额外类名，用于页面定制配色等。 */
  cardClass?: string
}>(), {
  title: '',
  maxWidth: 'min(80vw, 1180px)',
  maxHeight: '90vh',
  bordered: false,
  maskClosable: true,
  cardClass: '',
})

/*
 * 模板只能访问 setup 作用域的顶层绑定，所以这里把 props 解成独立常量。
 * 变量名不能与 prop 同名 —— 同名时 vue-tsc 会把模板里的标识符解析成
 * 组件实例属性（报 Property 'show' does not exist on ComponentPublicInstance）。
 */
const { show: visible, title: heading, bordered: hasBorder, maskClosable: closable, cardClass: extraClass } = toRefs(props)

const emit = defineEmits<{ (event: 'update:show', value: boolean): void }>()

const modalStyle = computed(() => ({
  width: props.maxWidth,
  maxWidth: props.maxWidth,
  height: props.maxHeight,
  maxHeight: props.maxHeight,
}))

/**
 * 内容区约束：min-height: 0 是让 flex 子项可收缩的关键，
 * 缺了它内容只会在自然高度上继续长，滚动条永远不会出现。
 */
const contentStyle = {
  flex: '1 1 auto',
  minHeight: '0',
  display: 'flex',
  flexDirection: 'column' as const,
  overflow: 'hidden',
}
</script>

<template>
  <n-modal :show="visible" preset="card" :title="heading || undefined" :bordered="hasBorder"
    :mask-closable="closable" class="app-modal" :class="extraClass" :style="modalStyle"
    :content-style="contentStyle" @update:show="(value: boolean) => emit('update:show', value)">
    <!-- 头部右侧附加内容（筛选器、计数等），透传 Naive 原生槽位 -->
    <template v-if="$slots['header-extra']" #header-extra>
      <slot name="header-extra" />
    </template>
    <!-- 唯一可滚动区域：内容多长都在这里，弹窗尺寸不变 -->
    <div class="app-modal-scroll">
      <slot />
    </div>
    <!-- 固定操作区：不传就完全不渲染，内容区自动占满剩余高度 -->
    <div v-if="$slots.footer" class="app-modal-footer">
      <slot name="footer" />
    </div>
  </n-modal>
</template>

<style scoped>
.app-modal-scroll {
  display: flex;
  flex-direction: column;
  gap: 12px;
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
  overscroll-behavior: contain;
  /* 给滚动条一点呼吸，避免贴着卡片内边距 */
  padding-right: 2px;
}
/*
 * 底部操作区固定在滚动区之外，只有它自己有上边框与内边距，
 * 所以内容滚到底时不会和按钮粘在一起。
 */
.app-modal-footer {
  display: flex;
  justify-content: flex-end;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
  padding-top: 12px;
  border-top: 1px solid var(--oc2-row-line);
  margin-top: 12px;
}
</style>
