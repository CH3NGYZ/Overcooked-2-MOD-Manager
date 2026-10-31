import { prefersReducedMotion } from './motion.ts'

/**
 * 卡片详情展开 / 收起后的滚动策略。
 *
 * 为什么需要它：详情用 `n-collapse-transition` 撑开时，卡片下面的内容会被顶下去，
 * 用户点的那张卡可能只剩半边。规则固定成三条，MOD 管理页与地图管理页共用：
 *
 * - **对齐对象是卡片外框**（`.mod-card` / `.package-shell` 的 border box），不是标题行。
 *   把标题行顶到边界会让卡片上边框、圆角与标题行上方那 10px 内边距被裁掉，
 *   看起来像「被切了一半」——这正是外框对齐要解决的问题。
 * - **展开**：底边留 `EXPANSION_BOTTOM_GAP` 的余量还能看全就不动；看不全就把卡片底边
 *   推到这条可用底边上；如果这样卡片外框会被顶到页头之上，退化成「外框贴页头下沿，
 *   其余向下溢出」。
 * - **收起**：把折叠后的这张卡滚到可视区**正中** —— 上下两张邻居都留在视口里，
 *   用户可以直接点下一张 / 上一张继续展开。对齐的是卡片外框中心，不是标题行。
 *
 * 顶部边界不是 0 而是页头下沿（`scrollInsetTop`）：页头固定、不随内容滚动，
 * 卡片贴到 0 会被页头压住。边界每次调用时实测，不缓存（页头会换行、缩放会变）。
 *
 * 只处理「用户自己点了展开 / 收起」这一次动作：因为展开另一张卡而被动收起的那张不滚
 * （地图管理页是单展开语义），跨页定位（`crossLink.ts`）与切页滚动（`pageTransition.ts`）
 * 也不受影响。
 *
 * 纯计算部分（`expansionScrollDelta` / `collapseScrollDelta`）不碰 DOM，Node 测试直接 import。
 */

/** 判定「已经贴住边界」的容差（px）：差这么点就不滚，避免一像素级的抖动。 */
const PIN_TOLERANCE = 2

/**
 * 「整卡可见」判定与「底边对齐」都留出的空隙（px）。
 *
 * 贴死视口底边时最后一行（例如「MOD 包 … 前置（2 个）」）会紧挨着边缘，
 * 看起来像被裁掉；12px 与卡片间距同量级，既有边界感又不浪费可视高度。
 */
export const EXPANSION_BOTTOM_GAP = 12

/**
 * 滚动缓动的时长（毫秒）：与 `n-collapse-transition` 的高度动画（0.3s）对齐。
 *
 * 不用原生 `scrollTo({ behavior: 'smooth' })` 的原因：详情撑开时页面的可滚动高度
 * 是**边动画边长出来**的，原生平滑滚动在调用瞬间就把目标夹到当时的 `scrollHeight`，
 * 于是滚到底部展开最后一张卡时只能停在旧底（实测差出 200 多像素），卡片底边贴不到
 * 视口底边。自己按帧推进，每帧把目标夹到当前可滚动范围，滚动就能跟着内容一起长出来。
 */
const SCROLL_DURATION = 320

export type ScrollReason = 'none' | 'reveal-bottom' | 'pin-top' | 'center'

export interface ExpansionFrame {
  /** 卡片外框（border box）顶边相对滚动容器可视区顶部的偏移。 */
  top: number
  /** 卡片外框的最终高度（含内边距与边框）。 */
  height: number
  /** 滚动容器的可视高度。 */
  viewportHeight: number
  /** 页头这类固定元素在滚动区顶部让出的高度；缺省 0（页头不在滚动区上方时）。 */
  insetTop?: number
  /** 底边对齐时留出的空隙；缺省 `EXPANSION_BOTTOM_GAP`。 */
  bottomGap?: number
  /** 容器当前 scrollTop。 */
  scrollTop: number
  /** 容器可用的最大 scrollTop，按**动画结束后**的内容高度算。 */
  scrollMax: number
}

export interface CollapseFrame {
  /**
   * 卡片外框（border box）顶边相对滚动容器可视区顶部的偏移。
   *
   * 必须在**折叠开始之前**量：折叠一开始卡片高度就在掉，这个位置不再代表展开态。
   */
  top: number
  /**
   * 折叠后的卡片外框高度：折叠前的高度减去详情内容的高度。
   *
   * 折叠一开始卡片就在掉高度，所以这也必须在改状态之前算好。
   */
  heightAfter: number
  /** 滚动容器的可视高度。 */
  viewportHeight: number
  /** 页头这类固定元素在滚动区顶部让出的高度；缺省 0。 */
  insetTop?: number
  /** 容器当前 scrollTop。 */
  scrollTop: number
  /** 收起后的可滚动上限（内容变短之后），用来夹住居中目标。 */
  scrollMaxAfter: number
}

export interface ScrollDelta {
  /** 相对当前 scrollTop 要移动的距离（正数向下滚，负数向上滚）。 */
  delta: number
  reason: ScrollReason
}

/** 把「相对位移」夹进容器的可滚动范围，返回真正能落地的位移。 */
function clampDelta(delta: number, scrollTop: number, scrollMax: number): number {
  const max = Math.max(0, scrollMax)
  const target = Math.min(Math.max(scrollTop + delta, 0), max)
  return target - scrollTop
}

/**
 * 展开卡片后的滚动距离。
 *
 * 1. `top + height` 还在「可用底边」（视口高度减去 `bottomGap`）之内：原地就能看全，不滚。
 * 2. 否则先把内容上移 `top + height - 可用底边`，让卡片底边贴住可用底边。
 * 3. 但这样卡片外框会被顶到 `insetTop` 之上时，改成只上移到「外框贴边界」：
 *    `target = min(delta, top - insetTop)`；这个上限小于 0（外框早就在边界之上，
 *    用户已经滚下去了）时取 0 —— 不再往前推，避免把用户拽走。
 * 4. 最后夹进容器的可滚动范围。
 */
export function expansionScrollDelta({
  top,
  height,
  viewportHeight,
  insetTop = 0,
  bottomGap = EXPANSION_BOTTOM_GAP,
  scrollTop,
  scrollMax,
}: ExpansionFrame): ScrollDelta {
  const usableBottom = viewportHeight - bottomGap
  const delta = top + height - usableBottom
  if (delta <= 0) return { delta: 0, reason: 'none' }
  // 最多只能把卡片顶推到「外框贴住页头下沿」，再往上卡片上边框与圆角就出界了。
  const limit = top - insetTop
  if (delta > limit) {
    return { delta: clampDelta(Math.max(0, limit), scrollTop, scrollMax), reason: 'pin-top' }
  }
  return { delta: clampDelta(delta, scrollTop, scrollMax), reason: 'reveal-bottom' }
}

/**
 * 收起卡片时的滚动距离：把折叠后的卡片外框滚到可视区**正中**。
 *
 * 上下各留一张邻居，收起后可以直接点下一张 / 上一张继续展开 —— 这是爸爸要的
 * 「同一个位置连着看几张详情」的效果。
 *
 * 居中 = 卡片顶边落在「页头下沿到视口底边」这段可视区里、扣掉卡片自己高度后剩下空间的
 * 一半处，卡片中心于是正好落在可视区中心。页头占掉的那一截不算可视区，所以目标是
 * `insetTop + (viewportHeight - insetTop - heightAfter) / 2`，不是整个视口的中线。
 *
 * 卡片比可视区还高（折叠后一般不会）也照公式居中，不退化成贴顶 —— 贴顶会把
 * 卡片的中心推到视口之外，用户还得自己找。已经居中（±2px）就不滚。
 *
 * 收起后内容变矮，可滚动上限也跟着变小：目标要夹进 `[0, scrollMaxAfter]`，
 * 短页面上尽量居中，被点那张卡的卡片外框不会跑到页头之上。
 */
export function collapseScrollDelta({
  top,
  heightAfter,
  viewportHeight,
  insetTop = 0,
  scrollTop,
  scrollMaxAfter,
}: CollapseFrame): ScrollDelta {
  // 居中 = 让卡片顶边落在「视口里给这张卡剩下的空间」的一半处；卡片中心于是正好在可视区中心。
  const centeredTop = insetTop + (viewportHeight - insetTop - heightAfter) / 2
  const delta = top - centeredTop
  if (Math.abs(delta) <= PIN_TOLERANCE) return { delta: 0, reason: 'none' }
  return { delta: clampDelta(delta, scrollTop, scrollMaxAfter), reason: 'center' }
}

/* ===== DOM 侧：量尺寸 + 执行滚动 ===== */

/** Naive 滚动条容器的类名，向上找不到可滚动祖先时的兜底。 */
const SCROLLBAR_CONTAINER = '.n-scrollbar-container'

/** 吸顶行的标记属性：用 data-* 而不是 class，避免和 Vue 的 `:class` 互相覆盖。 */
const STUCK_ATTRIBUTE = 'data-stuck'

/** 吸顶判定的容差（px）：滚动位置常带小数，钉住时也可能差半像素。 */
const STUCK_TOLERANCE = 1

/**
 * 从卡片向上找真正在滚动的祖先。
 *
 * 不能假设 `document.scrollingElement`：页面主滚动区是 `App.vue` 里的 `n-scrollbar`
 * （`.n-scrollbar-container`），页头固定、不参与滚动；模态框与抽屉里的卡片又各有各的容器。
 * 「真正滚动」= 纵向 overflow 可滚且内容确实超出可视区；都找不到时退回最近的
 * `.n-scrollbar-container`（内容没超出时它仍然是对的容器，`scrollMax` 为 0，算出来就是不滚）。
 */
export function resolveScrollContainer(element: Element): HTMLElement | null {
  let node: HTMLElement | null = element.parentElement
  while (node) {
    if (node.scrollHeight > node.clientHeight + 1) {
      const { overflowY } = getComputedStyle(node)
      if (overflowY === 'auto' || overflowY === 'scroll' || overflowY === 'overlay') return node
    }
    node = node.parentElement
  }
  const fallback = element.closest(SCROLLBAR_CONTAINER)
  return fallback instanceof HTMLElement ? fallback : null
}

/**
 * 页面顶部被固定元素占掉的高度（页头下沿相对滚动容器顶部的偏移）。
 *
 * 页面主滚动区上方是固定的 `.page-header`，卡片贴到容器顶就会被页头压住。
 * 每次调用实测：页头会因窗口宽度换行，界面缩放也会改变它。
 * 弹窗 / 抽屉里的卡片不在 `.app-main` 里，它们的边界就是自己的滚动容器顶部，返回 0。
 */
export function scrollInsetTop(container: HTMLElement): number {
  const shell = container.closest('.app-main')
  const header = shell?.querySelector('.page-header')
  if (!header) return 0
  return Math.max(0, header.getBoundingClientRect().bottom - container.getBoundingClientRect().top)
}

export interface StuckRowsTracker {
  /** 立刻重算一次：展开态变化、内容高度变化后调用（滚动与窗口尺寸会自动触发）。 */
  refresh(): void
  /** 解除监听。 */
  stop(): void
}

/**
 * 维护一组「展开后吸顶的标题行」的吸顶标记（`data-stuck`）。
 *
 * 不用 IntersectionObserver：吸顶行钉在边界上时**永远不会离开视口**，
 * 观察它自己分不出「钉住了」和「刚好滚到这里」；插哨兵元素又会动到页面 DOM。
 * 钉住时行顶与容器顶只差亚像素，每帧一次 rect 比较就够，行数是「当前展开的卡片 / 包」级别。
 *
 * 监听挂在 `document` 的捕获阶段：页面主滚动区是 Naive 的 `.n-scrollbar-container`，
 * 滚动事件不冒泡，捕获阶段才能不漏。不确定容器也能工作（每次重算时再解析）。
 */
export function trackStuckRows(
  rows: () => Array<Element | null | undefined>,
): StuckRowsTracker {
  let frame = 0
  const measure = () => {
    frame = 0
    const list = rows().filter((row): row is Element => !!row)
    if (!list.length) return
    const container = resolveScrollContainer(list[0])
    if (!container) return
    const boundary = container.getBoundingClientRect().top
    for (const row of list) {
      const rowTop = row.getBoundingClientRect().top
      row.toggleAttribute(STUCK_ATTRIBUTE, rowTop <= boundary + STUCK_TOLERANCE)
    }
  }
  const schedule = () => {
    if (frame) return
    if (typeof requestAnimationFrame !== 'function') {
      measure()
      return
    }
    frame = requestAnimationFrame(measure)
  }
  document.addEventListener('scroll', schedule, { capture: true, passive: true })
  window.addEventListener('resize', schedule)
  schedule()
  return {
    refresh: schedule,
    stop() {
      if (frame) cancelAnimationFrame(frame)
      frame = 0
      document.removeEventListener('scroll', schedule, { capture: true })
      window.removeEventListener('resize', schedule)
    },
  }
}

/**
 * 展开一张卡片：卡片外框用来看「当前高度与位置」，内容层用来推最终高度。
 *
 * `body` / `collapsing.body` 传的是被 `n-collapse-transition` 包住的那一层
 * （`.card-detail` / `.package-body`）。它的父节点是过渡包装层：展开动画只动包装层的
 * `max-height`，内容层自己的布局高度从一开始就是最终高度，所以「最终卡片高度 =
 * 当前卡片高度 - 包装层当前高度 + 包装层内容高度」。
 */
export interface ExpandScrollTarget {
  /** 卡片根元素（`.mod-card` / 包外壳 `.package-shell`）。 */
  card: Element
  /** 标题行元素（`.card-row` / `.package-head`）；只用来定位滚动容器，不参与对齐。 */
  anchor: Element
  /** 展开后的内容层；拿不到时按卡片当前高度处理。 */
  body: Element | null
  /**
   * 同一时刻因为「展开了这一张」而自动收起的上一张卡。
   *
   * 它位在目标卡片之前时，收起动画会把目标整体往上带：展开瞬间量到的 `top`
   * 还是收起前的位置，必须减掉它会缩掉的高度才是稳定后的位置。
   */
  collapsing?: { card: Element; body: Element | null } | null
}

/** 收起一张卡片时需要的元素。 */
export interface CollapseScrollTarget {
  /** 卡片根元素（`.mod-card` / 包外壳 `.package-shell`）：既是测量对象，也是展开记录的键。 */
  card: Element
  /** 折叠后的内容层（`.card-detail` / `.package-body`），用来估算收起后短掉多少高度。 */
  body?: Element | null
}

interface ViewportFrame {
  top: number
  viewportHeight: number
  scrollTop: number
  scrollMax: number
}

function viewportOf(container: HTMLElement): ViewportFrame {
  const rect = container.getBoundingClientRect()
  return {
    top: rect.top,
    viewportHeight: container.clientHeight,
    scrollTop: container.scrollTop,
    scrollMax: container.scrollHeight - container.clientHeight,
  }
}

/** 内容层的自然高度：包装层被 `max-height` 裁住时，`scrollHeight` 仍是完整内容高度。 */
function naturalContentHeight(body: Element): number {
  const wrapper = body.parentElement
  const content = body.getBoundingClientRect().height
  if (!wrapper) return content
  return Math.max(wrapper.scrollHeight, content)
}

/** 展开后的最终卡片高度：用包装层的自然内容高度补上还没长出来的那部分。 */
function expandedCardHeight(card: Element, body: Element | null): number {
  const cardHeight = card.getBoundingClientRect().height
  if (!body) return cardHeight
  const wrapper = body.parentElement
  if (!wrapper) return cardHeight
  return cardHeight - wrapper.getBoundingClientRect().height + naturalContentHeight(body)
}

/** 上一张卡收起时会把它下面的内容往上带多少像素；不在目标之前就是 0。 */
function collapseShrinkAbove(collapsing: NonNullable<ExpandScrollTarget['collapsing']>, target: Element): number {
  if (collapsing.card === target) return 0
  if (!collapsing.body) return 0
  const following = collapsing.card.compareDocumentPosition(target) & Node.DOCUMENT_POSITION_FOLLOWING
  return following ? naturalContentHeight(collapsing.body) : 0
}

/** 等一帧再量：详情是本帧刚插进 DOM 的，等多一帧能拿到动画起始状态，也避开 Vue 的补丁过程。 */
function afterFrame(run: () => void): void {
  if (typeof requestAnimationFrame === 'function') {
    requestAnimationFrame(() => run())
    return
  }
  setTimeout(run, 0)
}

/**
 * 展开后的滚动入口：等一帧 → 量 → 按 `expansionScrollDelta` 的结论滚。
 *
 * 调用方在 `nextTick()` 之后调用（此时详情已经渲染出来）；高度动画还在跑也没关系。
 */
export function scrollExpandedCardIntoView(target: ExpandScrollTarget): void {
  afterFrame(() => {
    const { card, anchor, body, collapsing } = target
    const container = resolveScrollContainer(anchor)
    if (!container) return
    const view = viewportOf(container)
    const cardRect = card.getBoundingClientRect()
    const shrink = collapsing ? collapseShrinkAbove(collapsing, card) : 0
    // 对齐卡片外框（border box），不再从标题行算起：上边框与圆角必须留在视口里。
    const top = cardRect.top - view.top - shrink
    const height = expandedCardHeight(card, body)
    // 滚动范围按「动画结束后」的内容高度算：这张卡会撑高这么多，被动收起的那张会缩掉那些。
    const projectedMax = Math.max(0, view.scrollMax + height - cardRect.height - shrink)
    const { delta } = expansionScrollDelta({
      top,
      height,
      viewportHeight: view.viewportHeight,
      insetTop: scrollInsetTop(container),
      scrollTop: view.scrollTop,
      scrollMax: projectedMax,
    })
    if (delta === 0) return
    applyScroll(container, view.scrollTop + delta)
  })
}

/**
 * 收起一张卡片：同步测量 → 改状态（`collapse`）→ 把折叠后的卡片滚到可视区正中。
 *
 * 顺序不能反。测量必须在改状态之前：折叠一开始卡片高度就在掉，量到的既不是
 * 展开态的位置也不是折叠后的高度。滚动则在折叠开始之后就启动，两者同时进行，
 * 用户看到的是「详情收起的同时页面把这张卡带到中间」。
 *
 * 只给「用户点了收起」的那一张用；被动收起（展开别的卡片让前一张收起）与
 * 批量收起不要调它 —— 那些场景里没有「用户正在看的那一张」。
 */
export function collapseCardWithScroll(target: CollapseScrollTarget, collapse: () => void): void {
  const { card, body } = target
  const container = resolveScrollContainer(card)
  const view = container ? viewportOf(container) : null
  const rect = view ? card.getBoundingClientRect() : null
  const detailHeight = body ? naturalContentHeight(body) : 0
  const top = rect ? rect.top - view!.top : 0
  const heightAfter = Math.max(0, (rect?.height ?? 0) - detailHeight)
  const insetTop = container ? scrollInsetTop(container) : 0
  const scrollMaxAfter = view ? Math.max(0, view.scrollMax - detailHeight) : 0
  collapse()
  if (!container || !view) return
  const { delta } = collapseScrollDelta({
    top,
    heightAfter,
    viewportHeight: view.viewportHeight,
    insetTop,
    scrollTop: view.scrollTop,
    scrollMaxAfter,
  })
  if (delta === 0) return
  applyScroll(container, view.scrollTop + delta)
}

/** 正在跑的滚动动画：新的一次滚动要先把它停掉，避免两段动画抢同一个 scrollTop。 */
let activeScroll: { cancel: () => void } | null = null

/** 等「内容长到目标位置可滚」的最长时间（毫秒）：与详情展开动画同量级，多留一点余量。 */
const REACHABLE_TIMEOUT = 420

function easeInOutCubic(progress: number): number {
  return progress < 0.5
    ? 4 * progress * progress * progress
    : 1 - ((-2 * progress + 2) ** 3) / 2
}

function scrollMaxOf(container: HTMLElement): number {
  return Math.max(0, container.scrollHeight - container.clientHeight)
}

/**
 * 立即滚动到目标；目标暂时滚不到（内容还在长高）时，等可滚动范围够了一次性跳过去。
 *
 * 减少动效的用户不能看缓动，但也不能在内容还没长出来时就赋值 —— 那一刻的
 * `scrollTop` 会被浏览器夹回旧底，卡片底边永远贴不到视口底边。
 */
function jumpScroll(container: HTMLElement, target: number): void {
  if (target <= scrollMaxOf(container) + 0.5) {
    container.scrollTop = Math.min(Math.max(target, 0), scrollMaxOf(container))
    return
  }
  const startedAt = performance.now()
  let frame = 0
  const cancel = () => {
    if (frame) cancelAnimationFrame(frame)
    frame = 0
    if (activeScroll?.cancel === cancel) activeScroll = null
  }
  const step = () => {
    const max = scrollMaxOf(container)
    if (max >= target - 0.5 || performance.now() - startedAt > REACHABLE_TIMEOUT) {
      container.scrollTop = Math.min(Math.max(target, 0), max)
      cancel()
      return
    }
    frame = requestAnimationFrame(step)
  }
  activeScroll = { cancel }
  frame = requestAnimationFrame(step)
}

function applyScroll(container: HTMLElement, target: number): void {
  activeScroll?.cancel()
  activeScroll = null
  if (prefersReducedMotion() || typeof requestAnimationFrame !== 'function') {
    jumpScroll(container, target)
    return
  }
  const start = container.scrollTop
  const distance = target - start
  if (Math.abs(distance) < 1) return
  const startedAt = performance.now()
  let lastSet = start
  let frame = 0

  const cancel = () => {
    if (frame) cancelAnimationFrame(frame)
    frame = 0
    if (activeScroll?.cancel === cancel) activeScroll = null
  }

  const step = (now: number) => {
    // 用户自己滚了（滚轮 / 触摸 / 滚动条）：让位，不再抢 scrollTop
    if (Math.abs(container.scrollTop - lastSet) > 2) {
      cancel()
      return
    }
    const progress = Math.min(1, (now - startedAt) / SCROLL_DURATION)
    const wanted = start + distance * easeInOutCubic(progress)
    // 每帧夹到「此刻」的可滚动范围：折叠动画还没结束时范围会比目标小
    const reachable = scrollMaxOf(container)
    container.scrollTop = Math.min(wanted, reachable)
    lastSet = container.scrollTop
    if (progress < 1) {
      frame = requestAnimationFrame(step)
      return
    }
    // 动画结束时范围应该已经稳定；还差一点就补上，避免停在一两像素外
    container.scrollTop = Math.min(Math.max(target, 0), scrollMaxOf(container))
    cancel()
  }

  activeScroll = { cancel }
  frame = requestAnimationFrame(step)
}
