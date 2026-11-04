<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { NButton } from 'naive-ui'
import { t } from '../i18n'
import {
  exitTutorial,
  measureTutorialTarget,
  nextStep,
  prevStep,
  scheduleMeasure,
  tutorial,
} from '../tutorial/engine'
import { placeCard, VIEWPORT_MARGIN, type Size } from '../tutorial/geometry'

/**
 * 教程覆盖层。
 *
 * ## 结构
 *
 * 一个 `position: fixed` 的根节点铺满视口并**接管全部点击**，里面是一圈
 * 高亮描边、一张卡片，以及没有目标时才用的整屏底板。所有装饰层都是
 * `pointer-events: none`：点击统一由根节点处理（点哪儿都算「继续」），
 * 这样「点高亮区域」与「点暗处」是同一套逻辑，不会因为洞的边界算出两种行为。
 *
 * ## 遮罩为什么是环的一层阴影，而不是四块面板
 *
 * 四块面板必须各自过渡 left/top/width/height，切步时四边速率不同，
 * 半透明底色在交界处会叠深、错开时会漏缝，屏幕上就是一层乱起来的暗块，
 * 高亮框尺寸变化越大越明显。改成让高亮环自己铺 `box-shadow: 0 0 0 100vmax`
 * 之后，遮罩与描边是同一个元素、同一份几何，过渡中绝不会拼错。
 *
 * 阴影不会被容器裁掉：环是浮层自己的 `position: fixed` 子元素，阴影属于
 * 浮层，与目标元素落在哪个 `overflow` 容器里无关。V1 之所以不敢这么做，
 * 是因为它把阴影加在**目标元素**上 —— 那才会被标题栏、侧栏、`n-scrollbar`
 * 的滚动容器裁出一道光。
 *
 * ## 输入
 *
 * - 点击任意位置 / 「下一步」：下一步
 * - 滚轮（累计到阈值才切，避免触控板一滑跳好几步）
 * - ← ↑ = 上一步，→ ↓ = 下一步，Esc = 退出
 * - 切步期间（`tutorial.busy`）全部忽略，避免连点开两次抽屉
 *
 * 滚轮与方向键都在 **window capture** 阶段监听并 `preventDefault`：教程期间
 * 页面不应该跟着滚，而这两类事件本来会先到 `n-scrollbar` 手上。
 */
const viewport = ref<Size>({ width: window.innerWidth, height: window.innerHeight })
const cardEl = ref<HTMLElement | null>(null)
/** 卡片高度只能量：文案长短随语言变化，估一个值会压到高亮框。 */
const cardHeight = ref(190)

const step = computed(() => tutorial.current.value)
const cardWidth = computed(() =>
  Math.min(step.value?.cardWidth ?? 380, Math.max(240, viewport.value.width - VIEWPORT_MARGIN * 2)),
)
const cardBox = computed(() =>
  placeCard(
    tutorial.rect.value,
    { width: cardWidth.value, height: cardHeight.value },
    viewport.value,
    step.value?.placement ?? 'auto',
  ),
)
const cardStyle = computed(() => ({
  left: `${cardBox.value.left}px`,
  top: `${cardBox.value.top}px`,
  width: `${cardWidth.value}px`,
}))
const progress = computed(() => {
  const total = tutorial.total.value
  if (!total) return 0
  return ((tutorial.index.value + 1) / total) * 100
})
const isLast = computed(() => tutorial.index.value >= tutorial.total.value - 1)
const title = computed(() => (step.value ? t(step.value.titleKey) : ''))
const body = computed(() => (step.value ? t(step.value.bodyKey) : ''))

/* ===== 卡片尺寸 ===== */

let cardObserver: ResizeObserver | null = null

function observeCard() {
  cardObserver?.disconnect()
  cardObserver = null
  if (!cardEl.value) return
  cardObserver = new ResizeObserver((entries) => {
    // 用元素自身的外框高度：contentRect 不含 padding，会让落点算矮 26px
    const height = entries[0]?.target.getBoundingClientRect().height
    if (height && Math.abs(height - cardHeight.value) >= 1) {
      cardHeight.value = Math.ceil(height)
    }
  })
  cardObserver.observe(cardEl.value)
  cardHeight.value = Math.ceil(cardEl.value.getBoundingClientRect().height)
}

/* ===== 视口与滚动 ===== */

function syncViewport() {
  viewport.value = { width: window.innerWidth, height: window.innerHeight }
}

function onScrollOrResize() {
  if (!tutorial.active.value) return
  syncViewport()
  scheduleMeasure()
}

/* ===== 点击 ===== */

function onBackdropClick() {
  nextStep()
}

/* ===== 滚轮 ===== */

/** 一次滚动手势的静默间隔：超过它算新手势，重新累计。 */
const WHEEL_GESTURE_GAP = 200
/** 累计到这么多像素才切一步（Windows 滚轮一格约 100px，触控板小得多）。 */
const WHEEL_STEP_DISTANCE = 45
/** 切一步后的锁定期，挡住同一手势的惯性尾巴。 */
const WHEEL_LOCK_MS = 260

let wheelAccumulator = 0
let wheelLastAt = 0
let wheelLockedUntil = 0

function onWheel(event: WheelEvent) {
  if (!tutorial.active.value) return
  // 教程期间页面不滚：滚轮在这里是切步手势
  event.preventDefault()
  const now = performance.now()
  if (now < wheelLockedUntil) return
  // 按行滚动的鼠标单位很小，换算成像素再累计
  const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? viewport.value.height : 1
  const delta = (Math.abs(event.deltaY) >= Math.abs(event.deltaX) ? event.deltaY : event.deltaX) * unit
  if (!delta) return
  if (now - wheelLastAt > WHEEL_GESTURE_GAP) wheelAccumulator = 0
  wheelLastAt = now
  // 反向立刻清零：来回抖一下不应该凑出一次切步
  if (wheelAccumulator !== 0 && Math.sign(delta) !== Math.sign(wheelAccumulator)) wheelAccumulator = 0
  wheelAccumulator += delta
  if (Math.abs(wheelAccumulator) < WHEEL_STEP_DISTANCE) return
  const forward = wheelAccumulator > 0
  wheelAccumulator = 0
  wheelLockedUntil = now + WHEEL_LOCK_MS
  if (forward) nextStep()
  else prevStep()
}

/* ===== 键盘 ===== */

function onKeydown(event: KeyboardEvent) {
  if (!tutorial.active.value) return
  // 带修饰键的组合留给系统/浏览器（缩放、切窗口等）
  if (event.altKey || event.ctrlKey || event.metaKey) return
  switch (event.key) {
    case 'ArrowRight':
    case 'ArrowDown':
      event.preventDefault()
      event.stopPropagation()
      nextStep()
      break
    case 'ArrowLeft':
    case 'ArrowUp':
      event.preventDefault()
      event.stopPropagation()
      prevStep()
      break
    case 'Escape':
      event.preventDefault()
      event.stopPropagation()
      exitTutorial()
      break
    default:
      break
  }
}

/* ===== 生命周期 ===== */

function bind() {
  window.addEventListener('resize', onScrollOrResize)
  // capture + passive:false：widen 到 n-scrollbar 之前，并能阻止页面滚动
  window.addEventListener('wheel', onWheel, { capture: true, passive: false })
  window.addEventListener('scroll', onScrollOrResize, { capture: true, passive: true })
  window.addEventListener('keydown', onKeydown, true)
}

function unbind() {
  window.removeEventListener('resize', onScrollOrResize)
  window.removeEventListener('wheel', onWheel, { capture: true })
  window.removeEventListener('scroll', onScrollOrResize, { capture: true })
  window.removeEventListener('keydown', onKeydown, true)
}

onMounted(bind)
onBeforeUnmount(() => {
  unbind()
  cardObserver?.disconnect()
  cardObserver = null
})

/* 每一步换卡片内容后：重挂尺寸观察、把焦点交给卡片（读屏器要念出来） */
watch(
  () => step.value?.id,
  async () => {
    wheelAccumulator = 0
    await nextTick()
    observeCard()
    measureTutorialTarget()
    cardEl.value?.focus({ preventScroll: true })
  },
)

/* 打开/关闭教程时同步一次视口，避免窗口在教程外被改过尺寸 */
watch(
  () => tutorial.active.value,
  (active) => {
    if (!active) return
    syncViewport()
    void nextTick().then(() => {
      observeCard()
      measureTutorialTarget()
    })
  },
)
</script>

<template>
  <Teleport to="body">
    <div v-if="tutorial.active.value && step" class="tutorial-root" @click="onBackdropClick">
      <!-- 没有目标时（引导卡）整屏铺一层底板；有目标时遮罩由高亮环的阴影接手 -->
      <div v-if="!tutorial.rect.value" class="tutorial-veil" />

      <!-- 高亮描边 + 遮罩：同一个元素，遮罩就是描边外那层阴影，共享同一份几何 -->
      <div v-if="tutorial.rect.value" class="tutorial-ring" :style="{
        left: `${tutorial.rect.value.left}px`,
        top: `${tutorial.rect.value.top}px`,
        width: `${tutorial.rect.value.width}px`,
        height: `${tutorial.rect.value.height}px`,
      }" />

      <div ref="cardEl" class="tutorial-card" :style="cardStyle"
        role="dialog" aria-modal="true" :aria-label="title" tabindex="-1" @click.stop @wheel.stop
        @keydown.stop>
        <div class="tutorial-card-head">
          <span class="tutorial-card-count"
            :aria-label="t('tutorial.stepProgress', { n: tutorial.index.value + 1, total: tutorial.total.value })">{{ tutorial.index.value + 1 }} / {{ tutorial.total.value }}</span>
          <n-button class="tutorial-card-skip" size="tiny" quaternary @click.stop="exitTutorial">
            {{ t('tutorial.skip') }}
          </n-button>
        </div>
        <div class="tutorial-card-progress" aria-hidden="true">
          <span :style="{ width: `${progress}%` }" />
        </div>
        <h3 class="tutorial-card-title">{{ title }}</h3>
        <p class="tutorial-card-body">{{ body }}</p>
        <div class="tutorial-card-foot">
          <span class="tutorial-card-hint">{{ t('tutorial.cardHint') }}</span>
          <div class="tutorial-card-actions">
            <n-button v-if="tutorial.index.value > 0" size="small" secondary @click.stop="prevStep">
              {{ t('tutorial.prev') }}
            </n-button>
            <n-button size="small" type="primary" @click.stop="nextStep">
              {{ isLast ? t('tutorial.done') : t('tutorial.next') }}
            </n-button>
          </div>
        </div>
      </div>
    </div>
  </Teleport>
</template>
