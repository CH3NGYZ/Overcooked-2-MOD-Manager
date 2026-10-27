<script setup lang="ts">
/**
 * 统一的应用弹窗外壳 —— 前端所有弹窗都走这里。
 *
 * 解决两个反复出现的布局问题：
 *   1. Naive 的 n-modal + preset="card" 高度由内容决定，内容一长就超出可视
 *      区域，底部按钮被顶到视口外；窗口本身不可滚动时直接够不到。
 *   2. 反过来，内容很短时又会被写成固定高度的弹窗拉出一大片空白。
 *
 * 规则：尺寸是「上限」而不是「定值」——
 *   宽：内容自然宽度，最多 max-width（默认 80vw / 1180px）
 *   高：内容自然高度，最多 max-height（默认 90vh）
 * 所以短内容自动收缩，长内容到上限后由中间区域滚动，
 * 头部与 #footer 始终固定，任何情况下都不会超出可视区域。
 *
 * 关键实现细节（都是踩过的坑）：
 *   · 高度约束必须用 :content-style 内联下发。Naive 的 .n-card-content 自带
 *     `flex: 1` 却**没有** `min-height: 0`，而 flex item 的 min-height 默认是
 *     auto —— 只靠外部 class 改不动，内容会把 card 撑破，并同时出现
 *     「弹窗不变高、内容被裁、明明溢出却滚不动」三个症状。
 *   · Naive 在 preset="card" 下把传入的 class 合并到「同时带 n-modal 与
 *     n-card 的同一个元素」上，内容区类名是 .n-card-content（不是
 *     .n-card__content）。
 */
import { computed, toRefs } from 'vue'
import { NModal } from 'naive-ui'

const props = withDefaults(defineProps<{
  show: boolean
  /** 卡片标题；不传则不渲染头部。 */
  title?: string
  /** 最大宽度（上限，不是定值）。 */
  maxWidth?: string
  /** 最大高度（上限，不是定值）。 */
  maxHeight?: string
  /** 内容区最小高度：内容很少时也不至于缩成一条，默认不设。 */
  minHeight?: string
  bordered?: boolean
  maskClosable?: boolean
  /**
   * `if` = 关闭时真正从 DOM 移除内容（Monaco 这类需要它，
   * 只 v-show 会让编辑器留在 0 尺寸容器里持续重排）。
   */
  displayDirective?: 'if' | 'show'
  /** 额外类名，用于页面定制配色等。 */
  cardClass?: string
  /**
   * 挂载目标。默认 `body`（全窗口居中、层级最高）。
   * 传选择器可把遮罩限制在某个容器内 —— 配置编辑器用它挂到
   * `.page-scroll`，让遮罩只覆盖侧栏右侧的内容区。
   */
  to?: string
}>(), {
  title: '',
  /*
   * 用百分比而不是 vw/vh：界面缩放是 WebView zoom，vw/vh 不随之缩放，
   * 高缩放下会算出比可见区域更大的尺寸。百分比相对已缩放的容器计算。
   */
  maxWidth: '80%',
  maxHeight: '90%',
  minHeight: '',
  bordered: false,
  maskClosable: true,
  displayDirective: 'if',
  cardClass: '',
  to: undefined,
})

const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  /**
   * 离场动画（Naive 的 fade-in-scale-up：缩小 + 渐隐）播完之后触发。
   * 宿主若要在关闭时清空内容（如丢弃预览、释放编辑器），应等到这里，
   * 否则内容会先消失、只剩一个空框在缩小，看不到渐隐。
   */
  (event: 'after-leave'): void
}>()

/*
 * 模板只能访问 setup 作用域的顶层绑定，所以把 props 解成独立常量。
 * 变量名不能与 prop 同名 —— 同名时 vue-tsc 会把模板里的标识符解析成
 * 组件实例属性（报 Property 'show' does not exist on ComponentPublicInstance）。
 */
const {
  show: visible,
  title: heading,
  maxWidth: widthLimit,
  maxHeight: heightLimit,
  minHeight: heightFloor,
  bordered: hasBorder,
  maskClosable: closable,
  displayDirective: directive,
  cardClass: extraClass,
  to: mountTarget,
} = toRefs(props)

/*
 * 全部用 max-*：宽高随内容自然伸缩，只在超过上限时才封顶。
 * max-height 生效后，内容区拿到确定高度，min-height: 0 才让它真正可滚。
 *
 * 宽度上限用百分比而不是 vw：Tauri 的界面缩放是 WebView zoom，`vw` 仍按
 * 未缩放的视口计算，200% 缩放下 80vw 会超出实际可见区域（截图里右边框
 * 跑到窗口外）。百分比相对 .n-modal-scroll-content（已受缩放约束）计算，
 * 任何缩放档位都落在可视范围内。
 */
const modalStyle = computed(() => ({
  width: 'auto',
  minWidth: 'min(320px, 92%)',
  maxWidth: widthLimit.value,
  height: 'auto',
  maxHeight: heightLimit.value,
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
    :mask-closable="closable" :display-directive="directive" :to="mountTarget" class="app-modal"
    :class="extraClass" :style="modalStyle" :content-style="contentStyle"
    @update:show="(value: boolean) => emit('update:show', value)"
    @after-leave="emit('after-leave')">
    <!-- 头部右侧附加内容（筛选器、计数等），透传 Naive 原生槽位 -->
    <template v-if="$slots['header-extra']" #header-extra>
      <slot name="header-extra" />
    </template>
    <!-- 唯一可滚动区域：内容再多也在这一层，弹窗尺寸不变 -->
    <div class="app-modal-scroll" :style="heightFloor ? { minHeight: heightFloor } : undefined">
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
