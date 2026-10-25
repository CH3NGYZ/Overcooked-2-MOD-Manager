import { nextTick, ref } from 'vue'

export interface LogicalRect { left: number; top: number; width: number; height: number }

const EXPAND_DURATION = 420
const SWITCH_DURATION = 300

/**
 * The app canvas is scaled by --ui-zoom, so getBoundingClientRect() reports
 * scaled pixels while inline left/top/width/height must be written in logical
 * (unscaled) pixels. Every rect that feeds an inline style goes through here.
 */
export function appContentScale(): number {
  const shell = document.querySelector('.app-shell')
  if (shell instanceof HTMLElement && shell.offsetWidth > 0) {
    const scale = shell.getBoundingClientRect().width / shell.offsetWidth
    if (Number.isFinite(scale) && scale > 0) return scale
  }
  return 1
}

export function logicalRect(element: Element): LogicalRect {
  const scale = appContentScale()
  const rect = element.getBoundingClientRect()
  return { left: rect.left / scale, top: rect.top / scale, width: rect.width / scale, height: rect.height / scale }
}

function lerpRect(from: LogicalRect, to: LogicalRect, t: number): LogicalRect {
  return {
    left: from.left + (to.left - from.left) * t,
    top: from.top + (to.top - from.top) * t,
    width: from.width + (to.width - from.width) * t,
    height: from.height + (to.height - from.height) * t,
  }
}

function setBox(element: HTMLElement, rect: LogicalRect) {
  element.style.left = `${rect.left}px`
  element.style.top = `${rect.top}px`
  element.style.width = `${rect.width}px`
  element.style.height = `${rect.height}px`
}

function clearBox(element: HTMLElement) {
  for (const name of ['left', 'top', 'width', 'height', 'overflow', 'transform', 'transition', 'transform-origin', 'opacity', 'z-index', 'border-radius', 'box-shadow', 'background', 'border-color']) {
    element.style.removeProperty(name)
  }
}

/**
 * 只动边框的 morph 动画（第二十九轮定稿模型）。
 *
 * 三种形态（紧凑列表 / 网格卡片 / 展开面板）是同一批 DOM，差别只在卡片盒
 * 的尺寸和 grid 区域。所以动画只做两件事：
 *   1. 逐帧改卡片盒的 left/top/width/height（真实布局尺寸，非 transform）；
 *   2. 高度/宽度变化时让内部的标题、图标、标签、版本、按钮、简介按新布局
 *      自然重排——绝不对单个元素做 scale/translate，否则就是"挤压缩放"。
 *
 * 帧驱动用 rAF + setTimeout 双保险；页面被遮挡（rAF 冻结）时直接瞬移终点。
 */
export function useRepositoryCardMorph(options: { computePanelRect: () => LogicalRect | null }) {
  const expandedId = ref<string | null>(null)
  const settledId = ref<string | null>(null)
  const closingId = ref<string | null>(null)
  const placeholderId = ref<string | null>(null)
  const placeholderHeight = ref(0)
  const morphing = ref(false)

  let expandedCard: HTMLElement | null = null
  let frameHandle = 0
  let frameTimer = 0
  // 展开期间被抬升层叠层级的祖先，收起时原样恢复。
  let liftedAncestors: Array<{ el: HTMLElement; isolation: string; zIndex: string }> = []

  function scheduleFrame(callback: (timestamp: number) => void) {
    let done = false
    const fire = (timestamp: number) => {
      if (done) return
      done = true
      callback(timestamp)
    }
    frameHandle = requestAnimationFrame(fire)
    frameTimer = window.setTimeout(() => fire(performance.now()), 40)
  }

  function stopFrameDrivers() {
    if (frameHandle) cancelAnimationFrame(frameHandle)
    if (frameTimer) clearTimeout(frameTimer)
    frameHandle = 0
    frameTimer = 0
  }

  function stopSwitchAnim() {
    stopFrameDrivers()
    document.querySelectorAll<HTMLElement>('.ocard').forEach((card) => {
      card.style.removeProperty('width')
      card.style.removeProperty('height')
      card.style.removeProperty('transform')
    })
  }

  /**
   * 卡片祖先带 isolation: isolate 会自建层叠上下文，卡片自己的 z-index:50
   * 无法越过吸顶标题（z-index:30）。展开时把整条祖先链的隔离打开并抬高。
   *
   * 只动 `isolation` / `z-index`，绝不改 `position`：
   * `.main-scroll` 里的 sticky 标题依赖自身的定位上下文，给祖先补
   * `position: relative` 会重建包含块让吸顶标题跳动（用户看到的"标题闪避"）。
   * 这些祖先本身已经是 position: relative/sticky（shell、group 都有定位），
   * 所以直接给 z-index 就足够生效。
   */
  function liftStackingChain(cardEl: HTMLElement) {
    releaseStackingChain()
    const list = cardEl.closest('.repository-list')
    let node: HTMLElement | null = cardEl.parentElement
    while (node) {
      liftedAncestors.push({
        el: node,
        isolation: node.style.isolation,
        zIndex: node.style.zIndex,
      })
      node.style.isolation = 'auto'
      node.style.zIndex = '60'
      if (node === list) break
      node = node.parentElement
    }
  }

  function releaseStackingChain() {
    for (const entry of liftedAncestors) {
      entry.el.style.isolation = entry.isolation
      entry.el.style.zIndex = entry.zIndex
    }
    liftedAncestors = []
  }

  function clearPartOpacity() {
    if (!expandedCard) return
    const descEl = expandedCard.querySelector<HTMLElement>(':scope > .odesc')
    const iconsEl = expandedCard.querySelector<HTMLElement>(':scope > .oline > .oacts > .oicons')
    const bodyEl = expandedCard.querySelector<HTMLElement>(':scope > .ocard-body')
    for (const el of [descEl, iconsEl, bodyEl]) {
      if (!el) continue
      el.style.removeProperty('opacity')
      el.style.removeProperty('transition')
      for (const name of ['position', 'left', 'top', 'width']) el.style.removeProperty(name)
    }
  }

  function finishClose() {
    const card = expandedCard
    if (card) {
      clearPartOpacity()
      clearBox(card)
    }
    releaseStackingChain()
    stopFrameDrivers()
    expandedCard = null
    expandedId.value = null
    settledId.value = null
    closingId.value = null
    placeholderId.value = null
    morphing.value = false
  }

  async function openCard(id: string, cardEl: HTMLElement) {
    if (morphing.value || expandedId.value || closingId.value) return
    if (cardEl.classList.contains('hide') || cardEl.classList.contains('search-hidden')) return
    stopSwitchAnim()

    const cardRect = logicalRect(cardEl)
    const iconsEl = cardEl.querySelector<HTMLElement>(':scope > .oline > .oacts > .oicons')

    expandedCard = cardEl
    morphing.value = true
    placeholderId.value = id
    placeholderHeight.value = Math.round(cardRect.height)
    expandedId.value = id
    await nextTick()
    if (expandedCard !== cardEl) return

    const panel = options.computePanelRect()
    if (!panel) {
      finishClose()
      return
    }

    liftStackingChain(cardEl)
    const reduce = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
    const finishOpen = () => {
      clearPartOpacity()
      setBox(cardEl, panel)
      settledId.value = id
      morphing.value = false
    }
    // 先把外框钉在卡片原位，再逐帧写到面板矩形；内部元素只跟随布局。
    setBox(cardEl, cardRect)
    if (reduce) {
      finishOpen()
      return
    }

    // Only the icon buttons fade in: the description preview never disappears,
    // so it needs no opacity animation of its own.
    if (iconsEl) { iconsEl.style.transition = 'none'; iconsEl.style.opacity = '0' }
    void cardEl.offsetWidth

    const ease = (p: number) => 1 - Math.pow(1 - p, 3)
    let started: number | null = null
    const frame = (now: number) => {
      if (started === null) started = now
      const p = Math.min(1, (now - started) / EXPAND_DURATION)
      if (document.hidden) {
        finishOpen()
        return
      }
      setBox(cardEl, lerpRect(cardRect, panel, ease(p)))
      if (p < 1) scheduleFrame(frame)
      else stopFrameDrivers()
    }
    scheduleFrame(frame)
    requestAnimationFrame(() => {
      if (iconsEl) { iconsEl.style.transition = `opacity 160ms ease ${Math.round(EXPAND_DURATION * 0.45)}ms`; iconsEl.style.opacity = '1' }
      frameTimer = window.setTimeout(() => {
        if (expandedCard === cardEl && expandedId.value === id) finishOpen()
      }, EXPAND_DURATION + 60)
    })
  }

  async function closeCard() {
    const cardEl = expandedCard
    const id = expandedId.value
    if (!cardEl || !id || morphing.value) return
    if (!cardEl.isConnected) {
      finishClose()
      return
    }
    const reduce = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
    if (reduce) {
      finishClose()
      return
    }

    morphing.value = true
    const boxNow = logicalRect(cardEl)
    const iconsEl = cardEl.querySelector<HTMLElement>(':scope > .oline > .oacts > .oicons')

    // 1) 进入收起态。`is-closing` 的排版尺寸全部由 `--ocard-close-p` 插值：
    //    0 = 展开面板尺寸，1 = 紧凑卡片尺寸。逐帧推进它，标题字号、图标、
    //    内边距、按钮内边距都会跟着外框一起连续缩小，而不是外框缩完再跳变。
    settledId.value = null
    expandedId.value = null
    closingId.value = id
    await nextTick()
    if (expandedCard !== cardEl) return
    cardEl.style.overflow = 'hidden'
    cardEl.style.setProperty('--ocard-close-p', '0')

    if (iconsEl) {
      iconsEl.style.transition = `opacity ${Math.round(EXPAND_DURATION * 0.3)}ms ease`
      iconsEl.style.opacity = '0'
    }

    // 2) 外框逐帧缩回占位符所在位置。占位符就是卡片该回到的槽位，直接用它
    //    的矩形；不能再叠加原始矩形差量，否则列表滚动后落点会偏移一张卡片。
    const placeholder = cardEl.previousElementSibling
    if (!(placeholder instanceof HTMLElement) || !placeholder.classList.contains('morph-placeholder')) {
      finishClose()
      return
    }
    const target = logicalRect(placeholder)
    const easeIn = (p: number) => p * p * p
    let started: number | null = null
    const frame = (now: number) => {
      if (started === null) started = now
      const p = Math.min(1, (now - started) / EXPAND_DURATION)
      if (document.hidden) {
        finishClose()
        return
      }
      const e = easeIn(p)
      setBox(cardEl, lerpRect(boxNow, target, e))
      cardEl.style.setProperty('--ocard-close-p', String(e))
      if (p < 1) scheduleFrame(frame)
      else stopFrameDrivers()
    }
    scheduleFrame(frame)
    frameTimer = window.setTimeout(() => {
      if (closingId.value === id) finishClose()
    }, EXPAND_DURATION + 40)
  }

  /**
   * 捕获卡片当前（旧布局）矩形。必须在布局真正切换前调用，例如响应式
   * watcher 的 pre 阶段；调用方随后 await nextTick() 再交给 animateSwitch。
   */
  function captureCards(container: HTMLElement): SwitchFlight[] {
    return Array.from(container.querySelectorAll<HTMLElement>('.ocard:not(.hide)'))
      .map((card) => ({ card, from: logicalRect(card), to: logicalRect(card), dx: 0, dy: 0 }))
  }

  /**
   * 列表↔网格切换的边框逐帧动画：调用前布局已经切换完成（nextTick 之后），
   * 这里把每张卡片钉回旧矩形，再逐帧真实缩放并跟随位置。元素零 transform。
   */
  async function animateSwitch(container: HTMLElement, flights: SwitchFlight[]) {
    if (morphing.value || expandedId.value || closingId.value) return false
    stopSwitchAnim()
    if (!flights.length || !flights.some((flight) => flight.card.isConnected)) return false
    const reduce = window.matchMedia?.('(prefers-reduced-motion: reduce)').matches
    if (reduce) return true

    morphing.value = true
    for (const flight of flights) {
      flight.to = logicalRect(flight.card)
      flight.dx = flight.from.left - flight.to.left
      flight.dy = flight.from.top - flight.to.top
      flight.card.style.width = `${flight.from.width}px`
      flight.card.style.height = `${flight.from.height}px`
      flight.card.style.transform = `translate(${flight.dx}px, ${flight.dy}px)`
    }
    const ease = (p: number) => (p < 0.5 ? 4 * p * p * p : 1 - Math.pow(-2 * p + 2, 3) / 2)
    let started: number | null = null
    const finish = () => {
      stopSwitchAnim()
      morphing.value = false
    }
    const frame = (now: number) => {
      if (started === null) started = now
      const p = Math.min(1, (now - started) / SWITCH_DURATION)
      // 页面隐藏时 rAF 冻结、定时器节流到秒级：直接瞬移终点，绝不慢放或钉死
      if (document.hidden) {
        finish()
        return
      }
      const e = ease(p)
      for (const flight of flights) {
        if (!flight.card.isConnected) continue
        flight.card.style.width = `${flight.from.width + (flight.to.width - flight.from.width) * e}px`
        flight.card.style.height = `${flight.from.height + (flight.to.height - flight.from.height) * e}px`
        const layout = logicalRect(flight.card)
        const layoutLeft = layout.left - flight.dx
        const layoutTop = layout.top - flight.dy
        const wantLeft = flight.from.left + (layoutLeft - flight.from.left) * e
        const wantTop = flight.from.top + (layoutTop - flight.from.top) * e
        flight.dx = wantLeft - layoutLeft
        flight.dy = wantTop - layoutTop
        flight.card.style.transform = `translate(${flight.dx}px, ${flight.dy}px)`
      }
      if (p < 1) scheduleFrame(frame)
      else finish()
    }
    scheduleFrame(frame)
    void container
    return true
  }

  function stopAll() {
    stopFrameDrivers()
    if (expandedCard) finishClose()
    stopSwitchAnim()
    morphing.value = false
  }

  return {
    expandedId, settledId, closingId, placeholderId, placeholderHeight, morphing,
    openCard, closeCard, captureCards, animateSwitch, stopAll,
  }
}

type SwitchFlight = {
  card: HTMLElement
  from: LogicalRect
  to: LogicalRect
  dx: number
  dy: number
}
