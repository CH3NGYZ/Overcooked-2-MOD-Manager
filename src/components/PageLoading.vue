<script setup lang="ts">
/**
 * 整页加载态：内容区正中一个转圈，圈下方可放一行/两行说明文字。
 *
 * ## 为什么不再用 `n-spin` 包住列表（四个整页页面以前的做法）
 *
 * Naive 的转圈是「在**自己的容器里**居中」，而那个容器的中心会随两件事漂移：
 *
 *   1. 页头高度 —— 每页不同（单个标题 vs 标题 + 一排操作），差 10 px 起；
 *   2. 内容总高度 —— 已经有数据时刷新，容器高度是「好几屏内容」而不是视口，
 *      转圈于是跑到滚动内容的中段，用户看到的是列表上半部分加一个偏下的圈。
 *
 * 同一份「正在加载」在不同页面因此落在不同高度上：这页看着在中央，那页偏上。
 *
 * 这里改成 fixed 相对窗口定位，只让出标题栏与侧栏：页面多长、页头多高都无关，
 * 转圈永远在内容可视区正中，四个整页页面完全一致。
 *
 * ## 与 `BusyOverlay` 的分工
 *
 * `BusyOverlay` 是**阻塞式**（盖住侧栏与标题栏、阻断一切交互），用于卸载/备份
 * 这类不能让用户走开的长操作。这里只覆盖内容区，用户仍能从侧栏切走 ——
 * 「加载失败想换个页面」不该被挡住。
 *
 * ## 用法
 *
 * 组件保留内容包裹（默认插槽原样渲染），所以页面只要把 `<n-spin :show="...">`
 * 换成 `<PageLoading :show="...">` 即可，内部结构不用动：
 *
 * ```html
 * <PageLoading :show="loading">
 *   <template #status>{{ t('common.loading') }}</template>
 *   <n-empty v-if="..." />
 *   <n-collapse v-else>…</n-collapse>
 * </PageLoading>
 * ```
 */
import { NSpin } from 'naive-ui'
import { t } from '../i18n'

defineProps<{
  /** 是否显示加载遮罩。内容包裹与它无关，始终渲染。 */
  show: boolean
}>()
</script>

<template>
  <slot />
  <Teleport to="body">
    <!--
      v-if 而不是 v-show：遮罩消失后不该留下任何参与命中测试的元素，
      否则下面的列表会短暂点不动（与 BusyOverlay 同一条理由）。
    -->
    <div v-if="show" class="page-loading" role="status" aria-live="polite" aria-busy="true">
      <n-spin size="large" />
      <!-- 文案槽：不传时用通用的「加载中…」，地图页会传逐份扫描进度 -->
      <div class="page-loading-text">
        <slot name="status">{{ t('common.loading') }}</slot>
      </div>
    </div>
  </Teleport>
</template>

<style scoped>
.page-loading {
  position: fixed;
  /* 只让出标题栏与侧栏：内容区 = 用户能看到的页面主体 */
  top: var(--oc2-titlebar-height);
  left: var(--oc2-rail-width);
  right: 0;
  bottom: 0;
  /* 页面内浮层：低于下载遮罩与 BusyOverlay，不挡其它全局浮层 */
  z-index: var(--oc2-layer-page, 2500);
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  /* 半透明而非全遮：用户能看出下面是什么页面，只是暂时还没读出来 */
  background: color-mix(in srgb, var(--oc2-body) 74%, transparent);
  backdrop-filter: blur(2px);
  cursor: progress;
}

/*
 * 转圈与文字的间距对齐 Naive 自己的 `.n-spin-description`（margin-top: 8px）：
 * 同一个数值只在这里写一次，改成自绘排版后仍然和平台观感一致。
 */
.page-loading-text {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  margin-top: 8px;
  font-size: 13px;
  line-height: 1.5;
  color: var(--oc2-text-2);
  pointer-events: none;
}
</style>
