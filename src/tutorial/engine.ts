import { computed, nextTick, ref, shallowRef } from 'vue'
import { router } from '../router'
import { clampRectToViewport, inflateRect, unionRects, type Rect, type Size } from './geometry'
import { startTutorialDemo, stopTutorialDemo } from './demo'
import { mergeWatchedSteps, normalizeStepIds, unwatchedSteps } from './progress'
import { tutorialSteps } from './steps'
import { clearTutorialProgress, isTutorialCompleted, markTutorialCompleted } from './storage'
import type { TutorialContext, TutorialStep } from './types'

/**
 * 教程运行时（模块级单例）。
 *
 * 引擎只做三件事：**当前是第几步、目标在哪、什么时候算讲完**。
 * DOM 事件（点击/滚轮/方向键）与卡片版式在 `TutorialOverlay.vue`，
 * 纯几何在 `geometry.ts`，步骤内容在各 `steps.*.ts` 里 —— 这里不出现
 * 任何一屏的具体文案或选择器。
 *
 * ## 为什么是单例
 *
 * 教程跨越路由（侧栏 → 总览 → MOD 安装 → …），状态不能挂在某个页面组件上：
 * 页面一切换实例就没了。与 `useDashboardStore`、`useModRepositoryRefresh` 同理，
 * 状态放模块级，`App.vue` 只负责挂载覆盖层。单例的副作用是**同一次运行只能有
 * 一份教程**，这也正是我们要的（重播时必须先把上一次结束掉）。
 *
 * ## 找不到目标怎么办
 *
 * 教程最容易坏的地方是选择器：目标没渲染、页面还在加载、列表为空。
 * 规则：
 *   - `optional: true` 的步骤直接跳过（这类步骤讲的是「可能不存在」的区域）；
 *   - 其余步骤退化成居中卡片继续讲，不中断 —— 一份教程卡在第 12 步出不来
 *     比少讲一步糟糕得多。
 *
 * ## 切步期间锁输入
 *
 * `runStep` 全程 `busy = true`：按钮、滚轮、方向键在收尾前一律忽略。
 * 否则连点两下会同时跑两条 prepare，抽屉被开两次、路由跳两次。
 */

/** 高亮框默认外扩：目标四周各留 6px，视觉上「框住」而不是「贴着」。 */
const DEFAULT_PADDING = 6
/** 等目标出现的最长时间（跨页/开抽屉后列表要时间渲染）。 */
const DEFAULT_TARGET_TIMEOUT = 2500
/** optional 步骤只等很短一下：没有就是没有，不要拖慢教程。 */
const OPTIONAL_TARGET_TIMEOUT = 400
/** 平滑滚动后等几何稳定的上限。 */
const SCROLL_SETTLE_TIMEOUT = 800
/**
 * 教程进度写盘前的合并窗口。
 *
 * 连续点「上一步/下一步」不该每步都打一次 IPC；走进新步骤后 400ms 内
 * 没有更多新步骤才写一次。结束运行时会立刻 flush，不等这个窗口。
 */
const PROGRESS_FLUSH_DELAY = 400

const active = ref(false)
const index = ref(0)
const total = ref(0)
const busy = ref(false)
/** 高亮框（视口 CSS px）；null = 不挖洞（纯讲解步骤或目标暂未就位）。 */
const rect = shallowRef<Rect | null>(null)

/** 本次运行的步骤快照（`when` 在开始时求值一次，运行中集合不再变化）。 */
let activeSteps: TutorialStep[] = []
/** 当前步骤的目标元素；重新测量时直接复用，不必再查选择器。 */
let targets: Element[] = []
/** 每次切步 +1；异步流程回来时对不上就说明已经过时，直接放弃。 */
let runToken = 0
let measureFrame = 0
/** 已经看过的步骤 id（启动时从 config 读入，运行中只增不减）。 */
let watchedStepIds: string[] = []
/** 把进度写进 config 的回调；`App.vue` 挂载后才有值。 */
let progressSink: ((ids: string[]) => void) | null = null
let progressTimer: ReturnType<typeof setTimeout> | null = null
let settleWaiters: Array<() => void> = []

function frame(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => resolve())
  })
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => {
    setTimeout(resolve, ms)
  })
}

function viewportSize(): Size {
  return { width: window.innerWidth, height: window.innerHeight }
}

/* ===== 目标解析 ===== */

function queryTargets(step: TutorialStep): Element[] {
  const target = step.target
  if (!target) return []
  let raw: Element | Element[] | null | undefined
  if (typeof target === 'string') {
    raw = Array.from(document.querySelectorAll(target))
  } else {
    try {
      raw = target()
    } catch {
      raw = null
    }
  }
  if (!raw) return []
  const list = Array.isArray(raw) ? raw : [raw]
  return list.filter((el): el is Element => Boolean(el) && el instanceof Element)
}

async function waitForTargets(step: TutorialStep, token: number): Promise<Element[]> {
  // 没有目标的步骤（引导卡、纯说明）不该等：等下只会白锁 2.5 秒输入
  if (!step.target) return []
  const timeout = step.optional ? OPTIONAL_TARGET_TIMEOUT : DEFAULT_TARGET_TIMEOUT
  const deadline = performance.now() + timeout
  let found = queryTargets(step)
  while (!found.length && performance.now() < deadline && token === runToken) {
    await frame()
    found = queryTargets(step)
  }
  return found
}

/**
 * 目标不在视口里就先滚过去，并等它停下来。
 *
 * 直接 `scrollIntoView` + 固定 `setTimeout` 不可靠：页面里有两套滚动容器
 * （`n-scrollbar` 的 `.page-scroll` 与弹窗自己的 `.app-modal-scroll`），
 * 平滑滚动的时长也不固定。这里改成「连续两帧位置不变才算稳定」，
 * 与 `expandScroll` 的落位对账同一个思路。
 */
async function waitForStablePosition(el: Element, timeoutMs = SCROLL_SETTLE_TIMEOUT): Promise<void> {
  const deadline = performance.now() + timeoutMs
  let previous = el.getBoundingClientRect()
  let stable = 0
  while (performance.now() < deadline) {
    await frame()
    const next = el.getBoundingClientRect()
    if (Math.abs(next.left - previous.left) < 0.5 && Math.abs(next.top - previous.top) < 0.5) {
      stable += 1
      if (stable >= 2) return
    } else {
      stable = 0
    }
    previous = next
  }
}

async function scrollTargetsIntoView(list: Element[]): Promise<void> {
  const el = list[0]
  if (!el) return
  const box = el.getBoundingClientRect()
  const size = viewportSize()
  const visible = box.top >= 0 && box.left >= 0 && box.bottom <= size.height && box.right <= size.width
  if (visible) return
  el.scrollIntoView({ block: 'center', inline: 'nearest', behavior: 'smooth' })
  await waitForStablePosition(el)
}

/* ===== 测量 ===== */

/** 变化小于半个像素不重写：亚像素抖动不该触发一次渲染。 */
const RECT_EPSILON = 0.5

function sameRect(a: Rect | null, b: Rect | null): boolean {
  if (!a || !b) return a === b
  return (
    Math.abs(a.left - b.left) < RECT_EPSILON &&
    Math.abs(a.top - b.top) < RECT_EPSILON &&
    Math.abs(a.width - b.width) < RECT_EPSILON &&
    Math.abs(a.height - b.height) < RECT_EPSILON
  )
}

/**
 * 按当前目标重算高亮框。
 *
 * 「当前」是字面意思：每次调用都重新读 `getBoundingClientRect()`，所以框是
 * 目标此刻几何的投影，而不是某一次定位的快照。`startFollow()` 每帧调它。
 *
 * 目标元素还可能被重渲染换掉（切 tabs、列表刷新、抽屉重开）：引用一旦全部
 * 脱离文档就按步骤选择器重找，否则会一直量一个已经不显示的旧节点。
 */
export function measureTutorialTarget(): void {
  if (!active.value) return
  if (targets.length && targets.every((el) => !el.isConnected)) {
    const step = tutorial.current.value
    targets = step ? queryTargets(step) : []
  }
  const boxes = targets
    .map((el) => el.getBoundingClientRect())
    .filter((box) => box.width > 0 || box.height > 0)
    .map((box) => ({ left: box.left, top: box.top, width: box.width, height: box.height }))
  const union = unionRects(boxes)
  const next = union
    ? clampRectToViewport(inflateRect(union, tutorial.current.value?.padding ?? DEFAULT_PADDING), viewportSize())
    : null
  if (!sameRect(next, rect.value)) rect.value = next
}

/** 每帧最多测一次：滚动事件会连续触发，直接测量会抖动。 */
export function scheduleMeasure(): void {
  if (measureFrame) return
  measureFrame = requestAnimationFrame(() => {
    measureFrame = 0
    measureTutorialTarget()
  })
}

/**
 * 逐帧跟随目标。
 *
 * 只在 `scroll` / `resize` 时重测是不够的 —— 高亮框会停在旧位置，与目标错开：
 *   - 抽屉滑入、面板切换这类布局变化走 `transform` 过渡，不触发任何事件；
 *   - 异步内容（README、版本列表）撑高面板时，元素整体平移；
 *   - 折叠卡展开/收起改变上方内容高度，目标被推着走。
 * 实测「手动选历史版本」这一步：抽屉从 x=1453 滑到 669（约 330 ms），
 * 元素刚进入视口就满足「完全可见」，于是在中途量了一次，抽屉最后又走了
 * 32 px，框就永远差那 32 px。
 *
 * 逐帧读一次 rect 是唯一能覆盖「动画中间帧」的办法：`ResizeObserver` 看不到
 * 位移，`MutationObserver` 看不到过渡帧。代价可控 —— 每帧 1~3 次
 * `getBoundingClientRect()`，静止时 `sameRect` 直接挡住，不产生任何渲染。
 */
let followFrame = 0

function startFollow(): void {
  if (followFrame) return
  const tick = () => {
    followFrame = 0
    // 切步期间 rect 被清空、目标也还没定位，跟随没有意义
    if (!active.value || busy.value) return
    measureTutorialTarget()
    followFrame = requestAnimationFrame(tick)
  }
  followFrame = requestAnimationFrame(tick)
}

function stopFollow(): void {
  if (followFrame) cancelAnimationFrame(followFrame)
  followFrame = 0
}

/* ===== 教程进度 ===== */

/**
 * 挂上「把进度写进 config」的回调；传 null 摘掉。
 *
 * 引擎不直接调 `api.saveSettings`：它只认识步骤表，不关心进度存在哪。
 * 由 `App.vue` 在挂载时接上配置读写；Node 测试不挂也能跑。
 */
export function setTutorialProgressSink(sink: ((ids: string[]) => void) | null): void {
  progressSink = sink
  // 先开始播放、后挂回调时，补写一次已经看到的步骤。
  if (sink && watchedStepIds.length > 0) emitProgress()
}

function scheduleProgressFlush(): void {
  if (!progressSink || progressTimer !== null) return
  progressTimer = setTimeout(() => {
    progressTimer = null
    emitProgress()
  }, PROGRESS_FLUSH_DELAY)
}

function emitProgress(): void {
  if (progressTimer !== null) {
    clearTimeout(progressTimer)
    progressTimer = null
  }
  progressSink?.(watchedStepIds)
}

/** 进入一步就记下 id；只有出现新 id 才排一次防抖写入。 */
function rememberStep(step: TutorialStep): void {
  const merged = mergeWatchedSteps(watchedStepIds, [step.id])
  if (merged.length === watchedStepIds.length) return
  watchedStepIds = merged
  scheduleProgressFlush()
}

/** 当前平台上真正会播的步骤（`when` 只在这里求值）。 */
function visibleSteps(): TutorialStep[] {
  return tutorialSteps.filter((step) => !step.when || step.when())
}

/* ===== 步骤执行 ===== */

/** `prepare` / `cleanup` 用的工具集；实现见 `types.ts` 的注释。 */
const context: TutorialContext = {
  click(selector, root) {
    const scope: ParentNode = root ?? document
    const el = scope.querySelector<HTMLElement>(selector)
    if (!el) return false
    el.click()
    return true
  },
  async waitFor(selector, timeoutMs = DEFAULT_TARGET_TIMEOUT, root) {
    const scope: ParentNode = root ?? document
    const deadline = performance.now() + timeoutMs
    let el = scope.querySelector(selector)
    while (!el && performance.now() < deadline) {
      await frame()
      el = scope.querySelector(selector)
    }
    return el
  },
  async waitForGone(selector, timeoutMs = DEFAULT_TARGET_TIMEOUT, root) {
    const scope: ParentNode = root ?? document
    const deadline = performance.now() + timeoutMs
    let el = scope.querySelector(selector)
    while (el && performance.now() < deadline) {
      await frame()
      el = scope.querySelector(selector)
    }
    return !el
  },
  wait: sleep,
  currentRoute() {
    const name = router.currentRoute.value.name
    return typeof name === 'string' ? name : null
  },
}

async function runStep(targetIndex: number): Promise<void> {
  const token = ++runToken
  const step = activeSteps[targetIndex]
  if (!step) return
  // 进入这一步就算看过；本次运行没走到、被「跳过」的步骤由 finishTutorial 兜底。
  rememberStep(step)
  stopFollow()
  busy.value = true
  rect.value = null
  try {
    if (step.route && context.currentRoute() !== step.route) {
      await router.push({ name: step.route }).catch(() => undefined)
      if (token !== runToken) return
      await nextTick()
    }
    if (step.prepare) {
      try {
        await step.prepare(context)
      } catch {
        // 准备动作失败不中断教程：目标找不到时下面会退化成居中卡片
      }
    }
    if (token !== runToken) return
    await nextTick()
    const found = await waitForTargets(step, token)
    if (token !== runToken) return
    targets = found
    if (found.length) {
      await scrollTargetsIntoView(found)
      if (token !== runToken) return
      // 「完全在视口里」不等于「已经不动」：抽屉滑入时元素一进视口就满足可见，
      // 之后还会平移几十像素。等几何稳定再量，框才会一次落在终位上。
      await waitForStablePosition(found[0])
      if (token !== runToken) return
      // 滚动刚落位的那一帧还没绘制，等一帧再量
      await frame()
      if (token !== runToken) return
    }
    measureTutorialTarget()
  } finally {
    if (token === runToken) {
      busy.value = false
      startFollow()
    }
  }
}

async function moveTo(targetIndex: number): Promise<void> {
  if (!active.value) return
  const target = activeSteps[targetIndex]
  if (!target) return
  const previous = activeSteps[index.value]
  if (previous && previous !== target && previous.cleanup) {
    try {
      await previous.cleanup(context)
    } catch {
      // 收尾失败不影响下一步
    }
  }
  if (!active.value) return
  index.value = targetIndex
  await runStep(targetIndex)
}

/* ===== 对外接口 ===== */

export const tutorial = {
  active,
  index,
  total,
  busy,
  rect,
  /** 当前步骤；未运行时为 null。 */
  current: computed<TutorialStep | null>(() => (active.value ? activeSteps[index.value] ?? null : null)),
}

export interface StartTutorialOptions {
  /** 从某一步开始（用于调试与「跳到这一节」）。 */
  fromStepId?: string
  /** 重新播放：清掉本地「已看过」标记，从头播全部步骤。 */
  restart?: boolean
  /** 已经看过的步骤 id；只播不在里面的步骤。缺省 = 一个都没看过。 */
  watchedStepIds?: readonly string[]
}

/**
 * 开始教程。已经在运行时直接返回（不会重入）。
 *
 * 默认只播「配置里没记过」的步骤：教程步骤表只增不改，升级后新增的步骤
 * 因此会在下次启动时单独播一遍，老步骤不再重复。用户主动重播（`restart`）
 * 与调试定位（`fromStepId`）要看完整流程，不按集合过滤。
 */
export function startTutorial(options: StartTutorialOptions = {}): void {
  if (active.value) return
  if (options.restart) clearTutorialProgress()
  const steps = visibleSteps()
  if (!steps.length) return
  const playAll = Boolean(options.restart || options.fromStepId)
  const playable = playAll ? steps : unwatchedSteps(steps, options.watchedStepIds ?? watchedStepIds)
  if (!playable.length) return
  // 本次运行的基准集合。从配置读进来时以它为准，避免上一次运行留下的 id
  // 让「这一步看没看过」判断串味；重播也保留集合（进入老步骤不该再写盘）。
  watchedStepIds = normalizeStepIds(options.watchedStepIds ?? watchedStepIds)
  activeSteps = playable
  total.value = playable.length
  const found = options.fromStepId
    ? playable.findIndex((step) => step.id === options.fromStepId)
    : 0
  index.value = found < 0 ? 0 : found
  targets = []
  // 示例卡片必须在第一步渲染前就位：本地空列表时先闪一帧空白态再补卡片会看到列表跳动。
  startTutorialDemo()
  active.value = true
  void runStep(index.value)
}

/**
 * 首次启动自动开始：只播配置里没记过的步骤。
 *
 * 返回是否真的开始了 —— 调用方据此决定要不要把启动刷新推迟到教程结束
 * （AGENTS.md 第 9 节的要求：教程期间不做真正的仓库/地图源刷新）。
 */
export function startTutorialIfFirstRun(
  watchedStepIdsFromConfig: readonly string[] = [],
): boolean {
  const watched = normalizeStepIds(watchedStepIdsFromConfig)
  /*
   * 本地「已看完」标记还在、配置里却没有记录：这是教程进度进 config 之前
   * 的老用户。把当前全部步骤一次性记下，否则升级后会从第 1 步完整重播。
   */
  if (isTutorialCompleted() && watched.length === 0) {
    watchedStepIds = visibleSteps().map((step) => step.id)
    emitProgress()
    return false
  }
  startTutorial({ watchedStepIds: watched })
  return active.value
}

export function nextStep(): void {
  if (!active.value || busy.value) return
  if (index.value >= activeSteps.length - 1) {
    finishTutorial()
    return
  }
  void moveTo(index.value + 1)
}

export function prevStep(): void {
  if (!active.value || busy.value || index.value <= 0) return
  void moveTo(index.value - 1)
}

/** 跳过 / 讲完 / Esc：都算「这次结束了」，并把标记写进本地存储。 */
export function exitTutorial(): void {
  if (!active.value) return
  finishTutorial()
}

function finishTutorial(): void {
  const last = activeSteps[index.value]
  stopFollow()
  active.value = false
  runToken += 1
  busy.value = false
  rect.value = null
  targets = []
  // settleWaiters 会被 App.vue 用来启动真实的仓库/地图源刷新，示例数据必须先撤掉。
  stopTutorialDemo()
  markTutorialCompleted()
  /*
   * 一次运行结束（讲完或跳过）就把这次的全部步骤算作已看过：
   * 跳过之后再启动不该从停下的那一步接着播 —— 那是「重新播放」的语义。
   */
  watchedStepIds = mergeWatchedSteps(
    watchedStepIds,
    activeSteps.map((step) => step.id),
  )
  emitProgress()
  if (last?.cleanup) {
    void Promise.resolve()
      .then(() => last.cleanup?.(context))
      .catch(() => undefined)
  }
  const waiters = settleWaiters
  settleWaiters = []
  for (const waiter of waiters) {
    try {
      waiter()
    } catch {
      // 观察者自己的错误不往引擎里抛
    }
  }
}

/**
 * 教程结束（讲完或跳过）后回调一次；没在运行就立刻回调。
 *
 * `App.vue` 用它把「启动时的仓库/地图源刷新」推迟到教程之后：
 * 教程一上来就会满屏刷新转圈、盖住要讲的卡片，而且用户正在读教程时
 * 后台刷新还会让卡片状态跳变。
 */
export function onTutorialSettled(callback: () => void): void {
  if (!active.value) {
    callback()
    return
  }
  settleWaiters.push(callback)
}

/** 重新播放（工具页「重新播放教程」入口）。 */
export function replayTutorial(): void {
  if (active.value) {
    exitTutorial()
  }
  startTutorial({ restart: true })
}

/** 供调试：当前步骤 id。 */
export function currentTutorialStepId(): string | null {
  return tutorial.current.value?.id ?? null
}
