<script setup lang="ts">
/**
 * 全屏阻塞遮罩（带转圈），用于**用户必须等待**的长操作。
 *
 * 为什么单独一个组件而不是各处写 CSS：
 *   1. 遮罩必须盖住整个窗口，包括侧栏、标题栏和页面滚动区 —— 只有
 *      `position: fixed` 才能做到，而这个选择在多个调用点必须一致；
 *   2. 必须阻止一切交互（点击侧栏切页、点按钮重复触发），否则用户在
 *      「备份/卸载正在写磁盘」时切走，会看到不一致的状态；
 *   3. 文案与层级只在这里定义一次，避免两个入口显示不同的措辞。
 *
 * 用 Naive 的 `n-spin` 承担旋转视觉，不自绘 keyframes。
 *
 * 注意：它会盖住自绘标题栏，所以**不能长时间停留** —— 只在确实需要
 * 等待的几十毫秒到几秒内出现，结束后立刻消失。标题栏拖动期间的鼠标
 * 事件被遮罩吞掉是可接受的，因为这个状态下不该让用户操作任何东西。
 */
import { NSpin } from 'naive-ui'

defineProps<{
  show: boolean
  /** 显示在转圈下方的说明文字，例如「正在备份 BepInEx 框架…」 */
  description?: string
}>()
</script>

<template>
  <Teleport to="body">
    <!--
      v-if 而不是 v-show：遮罩消失后不应留下任何参与命中测试的元素，
      否则它下面的页面会短暂点不动。
    -->
    <div v-if="show" class="busy-overlay" role="alert" aria-live="assertive" aria-busy="true">
      <n-spin size="large" :description="description" />
    </div>
  </Teleport>
</template>

<style scoped>
.busy-overlay {
  position: fixed;
  inset: 0;
  /* 层级取自 global.css 的浮层阶梯：高于下载遮罩，低于标题栏弹层 */
  z-index: var(--oc2-layer-busy, 4000);
  display: flex;
  align-items: center;
  justify-content: center;
  /* 半透明而非全遮：用户能看出程序还在，只是暂时不能操作 */
  background: color-mix(in srgb, var(--oc2-body) 78%, transparent);
  backdrop-filter: blur(2px);
  /* 遮住标题栏后不再需要让出顶部空间：这个状态本来就不允许操作 */
  cursor: progress;
}
</style>
