<script setup lang="ts">
/**
 * 本地条目卡片（MOD 管理页 / 地图管理页共用）。
 *
 * 外壳与样式来自 MOD 管理页原来的 `.mod-card`：左侧「开关 + 标题」，
 * 右侧「状态标签区（含可选快捷动作）| 工具栏」，点击卡片行展开详情。
 * 组件只负责**外壳与布局**：所有状态、文案、动作都由调用方算好后传入，
 * 组件不碰 `InstalledMod` / `LevelSetInfo` 任何一种模型 —— 一旦在这里
 * 判断业务类型，两页就会开始互相牵连。
 *
 * ## 插槽内容的样式属于页面
 *
 * `status` / `tools` / `detail` 三个插槽的内容由**父页面**编译，带的是页面的
 * scoped 作用域，组件里的 scoped 规则不会命中它们（反过来，页面也改不到
 * 卡片内部的开关与标题）。所以：
 * - `.tag-group`、状态标签的 `.state-tag--link` 等只出现在插槽里的样式由页面
 *   自己定义（MOD 管理页还要用同一套样式渲染多 DLL 包的标题行，保留在页面）；
 * - 组件只对插槽里两个**与列宽强耦合**的布局占位类负责：`.tool-divider`
 *   （组件自己渲染一条，父页面也可以在 status 插槽里放一条隔开快捷动作）
 *   与 `.tool-placeholder`（工具栏缺失按钮时的同宽占位）。
 *
 * ## 点击约定
 *
 * 根节点上的点击会 emit `activate`（详情区内的点击不算），父级据此决定
 * 展开 / 收起；开关与工具栏按钮必须 `@click.stop`，否则会连带触发展开。
 * 拆分前 MOD 管理页是在 `<article>` 上直接监听原生 `click`（用于给 document
 * 级「点空白收起」打标记），组件是单根节点，页面仍可这样监听原生 click。
 */
import { computed, useSlots } from 'vue'
import { NCollapseTransition, NSwitch } from 'naive-ui'
import type { CSSProperties } from 'vue'

const props = defineProps<{
  /** 开关当前值。 */
  enabled: boolean
  /** 开关的 loading（与该条目自己的异步操作绑定）。 */
  toggleLoading?: boolean
  /** 左侧标题（由调用方本地化并兜底）。 */
  title: string
  /** 标题的悬停提示，缺省用 title 自己。 */
  titleTooltip?: string
  /** 停用状态：整卡压暗。 */
  disabled?: boolean
  /** 详情是否展开（展开动画由组件内的 n-collapse-transition 负责）。 */
  expanded?: boolean
  /** 跨页跳转落点高亮。 */
  focusFlash?: boolean
  /** 页内按状态定位时的高亮。 */
  statusFlash?: boolean
  /** 工具栏最小列宽（页面按自己最多几个按钮算好，与下面两个值同源）。 */
  toolsMinWidth?: number
  /** 工具栏按钮间距，缺省由 CSS 回退到 8px。 */
  toolGap?: number
  /** 工具栏单个按钮宽度，缺省由 CSS 回退到 30px。 */
  toolButtonWidth?: number
}>()

const emit = defineEmits<{
  /** 开关变化：父级调后端并刷新。 */
  (event: 'toggle', value: boolean): void
  /**
   * 点击卡片行：父级自己决定展开 / 收起。
   *
   * 带上卡片根元素：页面要在展开 / 收起后按「优先向上展开」的规则滚动
   * （`src/expandScroll.ts`），需要这张卡的 DOM，而不是按 key 再查一遍。
   */
  (event: 'activate', card: HTMLElement): void
}>()

const slots = useSlots()

/**
 * 把列宽与间距以 CSS 变量下发，让样式和页面的列宽计算共用同一组数字。
 * 写死在两处（页面算列宽、卡片排间距）迟早会漂移。
 */
const toolGroupStyle = computed<CSSProperties>(() => {
  const style: CSSProperties = {}
  if (props.toolsMinWidth !== undefined) style.minWidth = `${props.toolsMinWidth}px`
  if (props.toolGap !== undefined) style['--tool-gap'] = `${props.toolGap}px`
  if (props.toolButtonWidth !== undefined) style['--tool-button-width'] = `${props.toolButtonWidth}px`
  return style
})

/**
 * 点击卡片行 → activate。
 *
 * 详情区内的点击不触发：详情区不拦冒泡，页面在 document 上监听「点空白
 * 收起」，这里再 toggle 一次就会「先展开、再收起」。同一个类名也是
 * 拆分前页面 `onCardClick` 的排除依据，不能改名。
 */
function onCardClick(event: MouseEvent) {
  const target = event.target
  if (target instanceof Element && target.closest('.card-detail')) return
  // 根节点是单元素 `<article>`，这里的 currentTarget 一定是卡片根
  emit('activate', event.currentTarget as HTMLElement)
}

function onToggle(value: boolean) {
  emit('toggle', value)
}
</script>

<template>
  <article class="mod-card"
    :class="{ disabled, expanded, 'focus-flash': focusFlash, 'status-flash': statusFlash }"
    @click="onCardClick">
    <!-- 单行：左「开关 + 标题」… 右「状态标签 | 工具栏」 -->
    <div class="card-row">
      <div class="card-left">
        <n-switch class="card-switch" size="small" :value="enabled" :loading="!!toggleLoading"
          @click.stop @update:value="onToggle" />
        <b class="card-title" :title="titleTooltip ?? title">{{ title }}</b>
      </div>

      <div class="card-right">
        <!--
          status 插槽装状态标签与可选快捷动作；没有标签区的行（例如多 DLL
          包的子卡片）不传这个插槽，用等宽的隐形分割线占位补回同一条竖线。
        -->
        <template v-if="slots.status">
          <slot name="status" />
          <!-- 分割线：把标签区和操作按钮区分开 -->
          <span class="tool-divider" />
        </template>
        <span v-else class="tool-divider-ghost" />

        <!-- 工具栏：可选按钮在最左（用占位保持列宽），固定按钮贴右 -->
        <span class="tool-group" :style="toolGroupStyle">
          <slot name="tools" />
        </span>
      </div>
    </div>

    <!-- 展开：用 Naive 自带的折叠过渡，不自己写动画 -->
    <n-collapse-transition :show="!!expanded" display-directive="show">
      <!-- 不阻止冒泡：点详情里的任意位置都算「点空白」，由页面的 document 监听收起 -->
      <div class="card-detail">
        <slot name="detail" />
      </div>
    </n-collapse-transition>
  </article>
</template>

<style scoped>
/* 卡片：单行三段 —— 左「开关 + 标题」，右「状态标签 + 工具栏」 */
.mod-card {
  display: flex;
  flex-direction: column;
  padding: 10px 14px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  /*
   * 底色走 --card-bg：调用方可以只换底色而不和下面的 hover / expanded
   * 比特异性 —— MOD 管理页的多 DLL 包外壳里，包内子卡片就是靠它用下沉底色
   * （`.package-body .mod-card { --card-bg: var(--oc2-sunken) }`）。
   */
  background: var(--card-bg, var(--oc2-card));
  cursor: pointer;
  transition: border-color .16s ease, background-color .16s ease, box-shadow .16s ease;
}
.mod-card:hover {
  border-color: var(--oc2-accent);
  background: var(--oc2-row-hover);
  box-shadow: 0 4px 16px rgba(0, 0, 0, .16);
}
.mod-card.expanded {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 8%, var(--oc2-card));
}
.mod-card.disabled { opacity: .62; }
/*
 * 定位高亮：3 秒后自动褪去（时长由 crossLink.ts 的 FOCUS_DURATION 控制）。
 * 两个类共用同一套视觉 —— focus-flash 是跨页跳转（单张），
 * status-flash 是页内聚合标签定位（可能同时命中多张）。
 *
 * 入场再播一次 pulse：跳转把用户「送」到新位置后，静态换色容易被
 * 正在进行的滚动掩盖；一次轻微的轮廓脉冲能确认落点。
 * 只做 opacity/box-shadow —— 不缩放卡片，否则会抖动相邻行布局。
 *
 * keyframes 名带组件前缀：keyframes 是全局的，和页面里给包外壳用的
 * `focus-pulse` 同名会互相覆盖。
 */
.mod-card.focus-flash,
.mod-card.status-flash {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 16%, var(--oc2-card));
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent);
  animation: local-item-card-focus-pulse 1200ms cubic-bezier(0.4, 0, 0.2, 1);
}
@keyframes local-item-card-focus-pulse {
  0%   { box-shadow: 0 0 0 0 color-mix(in srgb, var(--oc2-accent) 70%, transparent); }
  40%  { box-shadow: 0 0 0 5px color-mix(in srgb, var(--oc2-accent) 30%, transparent); }
  100% { box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent); }
}
@media (prefers-reduced-motion: reduce) {
  .mod-card.focus-flash,
  .mod-card.status-flash { animation: none; }
}
/*
 * 标题行**不吸顶**。
 *
 * 「标题不要顶出页面」由展开时的一次性滚动夹住（`expandScroll.ts` 的
 * `pin-top`：卡片外框最多推到页头下沿），而不是让标题行跟着滚动钉在顶部：
 * 真吸顶会盖住从下面滚过去的详情、还要给贴边态补顶边线，用户滑到一半
 * 反而分不清那是卡片标题还是页面元素。
 */
.card-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  min-width: 0;
}
.card-left {
  display: flex;
  align-items: center;
  gap: 11px;
  flex: 1;
  min-width: 0;
}
/*
 * 右侧一行三段：[状态标签组] | [分割线] | [工具栏]
 * 三段都垂直居中、不换行，工具栏宽度按全页最多按钮数固定，
 * 所以每个条目的按钮列都对齐在同一条竖线上。
 */
.card-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: nowrap;
  gap: 10px;
  flex: 0 0 auto;
  min-width: 0;
}
/*
 * 竖分割线：只做视觉分隔，不占额外空间。
 *
 * 用 :deep —— 标签区是 status 插槽内容（页面作用域），页面会在快捷动作
 * 按钮和被隔开的标签组之间放一条同样的分割线；组件自己渲染的那条在
 * `.card-right` 里，也会被这条规则命中，两处用同一份定义。
 */
.card-right :deep(.tool-divider) {
  width: 1px;
  height: 16px;
  flex: 0 0 1px;
  background: var(--oc2-row-line);
}
/*
 * 分割线的隐形占位：没有状态标签区的行（多 DLL 包的子卡片）不画竖线，
 * 但它必须占掉与 .tool-divider 相同的宽度（1px），否则父级 .card-right
 * 的 10px gap 会少一段，按钮列整体右移。
 */
.tool-divider-ghost {
  width: 1px;
  height: 1px;
  flex: 0 0 1px;
}
/*
 * 工具栏：按钮位等距排布。
 * gap 与按钮宽度来自页面下发的 CSS 变量（与列宽计算同源），
 * 所以样式和页面算出的列宽永远不会对不上。
 */
.tool-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--tool-gap, 8px);
  flex: 0 0 auto;
}
.tool-group :deep(.n-button) {
  width: var(--tool-button-width, 30px);
  min-width: var(--tool-button-width, 30px);
  padding: 0;
}
/* 没有可选按钮时保留同宽占位，保证按钮列在每个条目上都对齐 */
.tool-group :deep(.tool-placeholder) {
  width: var(--tool-button-width, 30px);
  height: 1px;
  flex: 0 0 var(--tool-button-width, 30px);
}
.card-switch { flex: 0 0 auto; }
.card-title {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--oc2-text-1);
  line-height: 1.35;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 展开：内容由 detail 插槽渲染，动画交给 n-collapse-transition */
.card-detail {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--oc2-row-line);
}
</style>
