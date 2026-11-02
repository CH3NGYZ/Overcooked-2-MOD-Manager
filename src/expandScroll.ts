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
 * 滚动容器与上下边界都可以由调用方显式声明（`container` / `bounds`）：弹窗的内容区
 * 在内容尚未溢出时不算「可滚动祖先」，启发式找不到它；弹窗也没有固定页头，顶部
 * 边界就是内容区自己的顶。声明之后同一套规则直接套进弹窗，不必另写一份滚法。
 *
 * 展开落位后还要对账一次（`settle`）：弹窗在滚动途中内容仍在长高，单次量到的
 * 可滚动上限会过时、滚到位时还差一截；对账只在用户没自己滚时补滚，最多两次。
 *
 * - **同时收起另一张**（地图管理页单展开）：被收起的那张在目标卡片之前时，收起会让
 *   目标整体上移 `shrink`。展开后的滚动位置要同步减掉这段，标题才留在原地、卡片从
 *   当前位置向下展开；只扣测量值不改滚动位置，卡片就会被上方那张的收起「挤上去」。
 *   浏览器自带的滚动锚定会再补偿一次、与我们的滚动互相抵消，所以 `.page-scroll` 的
 *   滚动容器关掉了 `overflow-anchor`（见 `global.css`）。
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
 * 判定「用户自己滚了」的容差（px）。
 *
 * 滚动动画每帧写下的位置自带亚像素误差，与最后一次写入差一两像素仍算它自己写的；
 * 超过才算用户抢走了滚动条，动画与落位对账都要让位。
 */
const USER_SCROLL_TOLERANCE = 2

/**
 * 落位对账最多补滚几次。
 *
 * 内容长高的动画（详情展开 0.3s）与滚动动画同量级，通常第一轮补滚就追平；
 * 留两次是给弹窗这类「滚动途中还在继续长高」的容器，再多就变成和用户抢滚动条。
 */
export const SCROLL_SETTLE_ROUNDS = 2

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
  /** 底边对齐时留出的空隙（距容器可视底）；缺省 `EXPANSION_BOTTOM_GAP`。 */
  insetBottom?: number
  /** `insetBottom` 的旧名，仍被既有页面与测试传递；两者都给时以 `insetBottom` 为准。 */
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
  /**
   * 底部让出的空隙（距容器可视底）；缺省 0。
   *
   * 收起是「把卡片居中」，上下本来对称，不需要像展开那样给底边留白；
   * 只有调用方显式声明了底部边界（例如容器里另有固定工具条）时才让出它。
   */
  insetBottom?: number
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
 * 底边余量取哪个值：`insetBottom` 是正式名，`bottomGap` 是早期的名字（既有调用方
 * 与测试还在传），两者都没有时才回到默认的 12px。
 */
function insetBottomOf(frame: Pick<ExpansionFrame, 'insetBottom' | 'bottomGap'>): number {
  return frame.insetBottom ?? frame.bottomGap ?? EXPANSION_BOTTOM_GAP
}

/**
 * 展开卡片后的滚动距离。
 *
 * 1. `top + height` 还在「可用底边」（视口高度减去 `insetBottom`）之内：原地就能看全，不滚。
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
  insetBottom,
  bottomGap,
  scrollTop,
  scrollMax,
}: ExpansionFrame): ScrollDelta {
  const usableBottom = viewportHeight - insetBottomOf({ insetBottom, bottomGap })
  const delta = top + height - usableBottom
  if (delta <= 0) return { delta: 0, reason: 'none' }
  // 最多只能把卡片顶推到「外框贴住页头下沿」，再往上卡片上边框与圆角就出界了。
  const limit = top - insetTop
  if (delta > limit) {
    return { delta: clampDelta(Math.max(0, limit), scrollTop, scrollMax), reason: 'pin-top' }
  }
  return { delta: clampDelta(delta, scrollTop, scrollMax), reason: 'reveal-bottom' }
}

/** 展开卡片时真正要滚到的 `scrollTop` 的输入。 */
export interface ExpansionTargetFrame extends ExpansionFrame {
  /** 展开前卡片自己的高度（`cardRect.height`）：滚动上限要按「长高了多少」修正。 */
  cardHeight: number
  /** 上方那张卡收起会让内容整体上移的距离；已在改状态之前量好，缺省 0。 */
  shrink?: number
}

/**
 * 展开卡片后的目标 `scrollTop`。
 *
 * 两步不能合并：`expansionScrollDelta` 回答的是「从标题现在的位置出发，还要往下滚多少
 * 才能让卡片底边贴住可用底边」；而上方那张卡收起会让内容整体上移 `shrink`，等于替我们
 * 滚了 `shrink`。所以目标位置是 `scrollTop + delta - shrink`。
 *
 * 反过来（把 `shrink` 预先扣进 `top` 再算 delta）等价于假定「滚动位置不动、内容自己上移」：
 * 卡片会被上方那张的收起拽上去 —— 用户看到的就是「标题被挤上去、不是向下展开」。
 *
 * `shrink` 是「这次测量时还没在几何里体现的收缩量」，不是常量：收缩一旦发生，卡片位置与
 * 可滚动上限都已经含这段位移，重算必须传 0。
 */
export function expansionTargetScrollTop({
  top,
  height,
  cardHeight,
  shrink = 0,
  viewportHeight,
  insetTop = 0,
  insetBottom,
  bottomGap,
  scrollTop,
  scrollMax,
}: ExpansionTargetFrame): number {
  const usableBottom = viewportHeight - insetBottomOf({ insetBottom, bottomGap })
  // 上方那张收起后，这张卡的外框在**内容坐标**里的位置（不随滚动变）。
  const contentTop = scrollTop + top - shrink
  // 标题留在原地时的滚动位置：上方替我们滚了 shrink，这里就要减掉它。
  const restingTop = contentTop - top
  // 从标题现在的位置看，底边还要往下露多少才能贴住可用底边。
  const overflow = top + height - usableBottom
  const revealing = overflow > 0 ? restingTop + overflow : restingTop
  // 但不能把标题顶到上界之上（触顶后退化成向下溢出）；
  // 标题早就在上界之上时不往上抢，就停在原地。
  const capped = Math.max(restingTop, Math.min(revealing, contentTop - insetTop))
  // 动画结束后的可滚动上限：这张卡撑高 `height - cardHeight`，上面那张缩掉 `shrink`。
  const scrollMaxAfter = Math.max(0, scrollMax + height - cardHeight - shrink)
  return Math.min(Math.max(capped, 0), scrollMaxAfter)
}

/**
 * 收起卡片时的滚动距离：把折叠后的卡片外框滚到可视区**正中**。
 *
 * 上下各留一张邻居，收起后可以直接点下一张 / 上一张继续展开 —— 这是爸爸要的
 * 「同一个位置连着看几张详情」的效果。
 *
 * 居中 = 卡片顶边落在「页头下沿到视口底边」这段可视区里、扣掉卡片自己高度后剩下空间的
 * 一半处，卡片中心于是正好落在可视区中心。页头与容器底部的固定工具条占掉的两截
 * 不算可视区，所以目标是 `insetTop + (viewportHeight - insetTop - insetBottom - heightAfter) / 2`，
 * 不是整个视口的中线。
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
  insetBottom = 0,
  scrollTop,
  scrollMaxAfter,
}: CollapseFrame): ScrollDelta {
  // 居中 = 让卡片顶边落在「视口里给这张卡剩下的空间」的一半处；卡片中心于是正好在可视区中心。
  // 底部让出的空隙同样从可用高度里扣掉，否则卡片中心会偏下。
  const centeredTop = insetTop + (viewportHeight - insetTop - insetBottom - heightAfter) / 2
  const delta = top - centeredTop
  if (Math.abs(delta) <= PIN_TOLERANCE) return { delta: 0, reason: 'none' }
  return { delta: clampDelta(delta, scrollTop, scrollMaxAfter), reason: 'center' }
}

/** 落位对账的输入：用的是哪次滚动的落点、哪次测量的上限。 */
export interface ScrollSettleFrame {
  /** 当前 scrollTop。 */
  scrollTop: number
  /** 这次滚动最后一次写下的 scrollTop。 */
  lastSet: number
  /** 现在容器的可滚动上限。 */
  scrollMaxNow: number
  /** 计算目标时用的可滚动上限。 */
  scrollMaxUsed: number
  /** 已经补滚过几次。 */
  rounds: number
  /** 最多补滚几次；缺省 `SCROLL_SETTLE_ROUNDS`。 */
  maxRounds?: number
}

/**
 * 落位之后是否要再补滚一次。
 *
 * 展开动画与滚动同时进行：滚动途中内容还在变高，算目标时量到的可滚动上限会过时，
 * 于是滚到位时底边还差一截（弹窗里展开随包插件最明显）。两种情况不补滚：
 *
 * - 用户自己滚了（当前位置与最后一次写入差出容差）：让位，不再抢滚动条；
 * - 容器没有比计算时更高（含已达补滚上限）：目标没被夹短，再滚只是空转。
 */
export function needsScrollSettle({
  scrollTop,
  lastSet,
  scrollMaxNow,
  scrollMaxUsed,
  rounds,
  maxRounds = SCROLL_SETTLE_ROUNDS,
}: ScrollSettleFrame): boolean {
  if (rounds >= maxRounds) return false
  if (Math.abs(scrollTop - lastSet) > USER_SCROLL_TOLERANCE) return false
  // 0.5px 容差：scrollHeight 取整、clientHeight 可能带小数，亚像素变化不算内容长高。
  return scrollMaxNow > scrollMaxUsed + 0.5
}

/* ===== DOM 侧：量尺寸 + 执行滚动 ===== */

/** Naive 滚动条容器的类名，向上找不到可滚动祖先时的兜底。 */
const SCROLLBAR_CONTAINER = '.n-scrollbar-container'

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

/**
 * 显式声明滚动层的属性名。
 *
 * 弹窗内容区（`.app-modal-scroll`）在内容尚未溢出时不满足「scrollHeight > clientHeight」，
 * 启发式找不到它；用属性声明后，即使卡片还没把内容撑高也能拿到正确的滚动容器。
 */
const SCROLL_CONTAINER_ATTR = 'data-scroll-container'

/** 上下边界的数值口径：数字直接是 px，元素用自己的实测边（见 `resolveScrollInsets`）。 */
export interface ScrollBounds {
  /** 顶部边界：数字 = 相对容器可视顶的 px；元素 = 它的底边（固定页头、工具条）。 */
  top?: number | HTMLElement
  /** 底部边界：数字 = 距容器可视底的留白 px；元素 = 它的顶边（固定页脚、工具条）。 */
  bottom?: number | HTMLElement
}

export interface ScrollInsets {
  /** 距容器可视顶的 px。 */
  top: number
  /** 距容器可视底的 px。 */
  bottom: number
}

/** `bounds` 没给边时用的底边余量：展开按卡片间距留白，收起是居中、不额外留。 */
export interface ScrollInsetDefaults {
  bottom?: number
}

/**
 * 把 `bounds` 统一换成「距容器可视顶 / 距容器可视底」的 px。
 *
 * 元素边界在调用时实测相对容器的位置：页面里的固定页头会因窗口宽度换行、界面缩放
 * 也会改变它，弹窗里的工具条还可能被别的内容挤动，缓存下来就会错。落在容器之外
 * （算出来是负值）按 0 算，不让卡片滑出容器。
 */
export function resolveScrollInsets(
  container: HTMLElement,
  bounds: ScrollBounds = {},
  defaults: ScrollInsetDefaults = {},
): ScrollInsets {
  const rect = container.getBoundingClientRect()
  const top = bounds.top === undefined
    ? scrollInsetTop(container)
    : typeof bounds.top === 'number'
      ? Math.max(0, bounds.top)
      : Math.max(0, bounds.top.getBoundingClientRect().bottom - rect.top)
  const bottom = bounds.bottom === undefined
    ? (defaults.bottom ?? EXPANSION_BOTTOM_GAP)
    : typeof bounds.bottom === 'number'
      ? Math.max(0, bounds.bottom)
      : Math.max(0, rect.bottom - bounds.bottom.getBoundingClientRect().top)
  return { top, bottom }
}

/**
 * 卡片所在滚动层的解析顺序：显式声明 → `data-scroll-container` 标记 → 「谁真的在滚」。
 *
 * 最后一步是页面场景的兜底（页面滚动区没有标记，靠内容溢出判定）；弹窗与抽屉用属性
 * 声明，内容还没溢出时也能拿到容器，不必等它先溢出才发现自己找错了层。
 */
function scrollContainerOf(card: Element, declared?: HTMLElement | null): HTMLElement | null {
  if (declared) return declared
  const marked = card.closest(`[${SCROLL_CONTAINER_ATTR}]`)
  if (marked instanceof HTMLElement) return marked
  return resolveScrollContainer(card)
}

/**
 * 按展开键找卡片。键由调用方决定（文件名 / 目录名，内层键还带父键前缀），直接落在
 * `data-expand-key` 属性上；键里可能有引号或控制字符，拼进选择器前必须 `CSS.escape`。
 * `root` 供另一页在局部子树里查找（同屏既有页面又有弹窗时不会串到别的树里）。
 */
export function cardByExpandKey(key: string, root: ParentNode = document): HTMLElement | null {
  const found = root.querySelector(`[data-expand-key="${CSS.escape(key)}"]`)
  return found instanceof HTMLElement ? found : null
}

/*
 * 这里**没有**「吸顶行贴边标记」这类跟踪器。
 *
 * 展开后标题行不吸顶（`LocalItemCard` / 包外壳都不再 `position: sticky`），
 * 卡片详情滚到页头下沿这件事只由 `expansionScrollDelta` 的一次性滚动夹住：
 * 用户滚下去以后标题就跟着内容走，不再需要实时判断「这一行有没有钉住」。
 */

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
  /**
   * 标题行元素（`.card-row` / `.package-head`）。
   *
   * 它坐在卡片内部、与卡片同处一个滚动层；保留这个字段是因为页面调用点还在传
   * （包外壳与子卡共用一个调用形状）。对齐与量高一律用 `card`，容器也不再从它定位。
   */
  anchor: Element
  /** 展开后的内容层；拿不到时按卡片当前高度处理。 */
  body: Element | null
  /** 显式声明的滚动容器；缺省按 `scrollContainerOf` 的顺序解析。 */
  container?: HTMLElement | null
  /**
   * 上下边界。弹窗没有固定页头，顶部边界就是内容区自己的顶（写 0）；
   * 页面缺省即可（顶部用页头下沿、底部留 `EXPANSION_BOTTOM_GAP`）。
   */
  bounds?: ScrollBounds
  /** 落位对账是否开启，默认开（内容在滚动途中继续变高时补滚）。 */
  settle?: boolean
  /**
   * 同一时刻因为「展开了这一张」而自动收起的上一张卡。
   *
   * 它位在目标卡片之前时，收起动画会把目标整体往上带 `shrink`：目标滚动位置要同步
   * 减掉这段（`expansionTargetScrollTop` 负责），标题才留在原地、卡片向下展开。
   *
   * 这段补偿**只兑现一次**：收缩真的发生后，几何（卡片位置、可滚动上限）本身就含
   * 这段位移，重算再减就是补偿两遍 —— 卡片会被反着滚回去。兑现交给
   * `expansionPlanOf` 里的 `takeOnce`。
   */
  collapsing?: {
    card: Element
    body: Element | null
    /**
     * 已经量好的「这张卡会缩掉多少高度」。
     *
     * 默认在滚动前现量；但调用方如果在改状态的同一步里把这张卡的内层展开状态
     * 一起清掉（外层收起连带清内层），内容会先变矮，现量就会低估 —— 那种情况
     * 必须在改状态之前用 `naturalContentHeight` 量好传进来。
     */
    shrink?: number
  } | null
}

/** 收起一张卡片时需要的元素。 */
export interface CollapseScrollTarget {
  /** 卡片根元素（`.mod-card` / 包外壳 `.package-shell`）：既是测量对象，也是展开记录的键。 */
  card: Element
  /** 折叠后的内容层（`.card-detail` / `.package-body`），用来估算收起后短掉多少高度。 */
  body?: Element | null
  /** 显式声明的滚动容器；缺省按 `scrollContainerOf` 的顺序解析。 */
  container?: HTMLElement | null
  /** 上下边界；收起是居中，底部只在调用方显式声明时才让出。 */
  bounds?: ScrollBounds
  /** 落位对账是否开启，默认开。 */
  settle?: boolean
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

/**
 * 内容层的自然高度：包装层被 `max-height` 裁住时，`scrollHeight` 仍是完整内容高度。
 *
 * 调用方只在**改状态之前**量：折叠一开始内容就在变矮，之后量到的既不是折叠前
 * 的高度，也不是折叠后的（后者恒为 0）。
 */
export function naturalContentHeight(body: Element): number {
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

/**
 * 上一张卡收起时会把它下面的内容往上带多少像素；不在目标之前就是 0。
 *
 * 只在**改状态那一刻**调用：那时上一张卡还摊开着，量到的是收缩前的位置。滚动一开始
 * 收缩就在真的发生，之后再量已经晚了一拍 —— 所以这个值要交给 `takeOnce` 兑现一次。
 */
function collapseShrinkAbove(collapsing: NonNullable<ExpandScrollTarget['collapsing']>, target: Element): number {
  if (collapsing.card === target) return 0
  const following = collapsing.card.compareDocumentPosition(target) & Node.DOCUMENT_POSITION_FOLLOWING
  if (!following) return 0
  if (collapsing.shrink !== undefined) return collapsing.shrink
  return collapsing.body ? naturalContentHeight(collapsing.body) : 0
}

/**
 * 一次性数值：第一次取走原值，之后恒为 0。
 *
 * 上一张卡收起带来的位移只在改状态之前量得准；滚动一开始，那张卡在真的收，卡片位置与
 * 可滚动上限都跟着变。把它当常量每次重算都减，就是补偿两遍 —— 目标会被算到当前滚动位置
 * 之上，卡片反着滚回去（实测 −588px，约等于上一张卡的详情高度）。所以补偿是一张用一次
 * 就作废的凭证，不是每次重算都要减的常量。
 */
export function takeOnce(value: number): () => number {
  let left = value
  return () => {
    const current = left
    left = 0
    return current
  }
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
 * 展开后的滚动入口：等一帧 → 量 → 按 `expansionScrollDelta` 的结论滚，落位后对账。
 *
 * 调用方在 `nextTick()` 之后调用（此时详情已经渲染出来）；高度动画还在跑也没关系，
 * 目标每帧夹到当前可滚动范围，落位后由 `settle` 补滚。
 */
export function scrollExpandedCardIntoView(target: ExpandScrollTarget): void {
  afterFrame(() => {
    const plan = expansionPlanOf(target)
    if (!plan) return
    if (Math.abs(plan.target - plan.container.scrollTop) < PIN_TOLERANCE) return
    scrollAndSettle(plan, target.settle !== false)
  })
}

/**
 * 展开的滚动计划：每次重算都重新量卡片与容器。
 *
 * 展开动画期间卡片在长高、容器可滚动上限也在变，所以对账不能复用第一次的几何。
 * 只有「上一张卡收起带来的位移」不能重算 —— 它属于改状态那一刻，用 `takeOnce`
 * 兑现一次；之后的测量看到的是已经含这段位移的几何。
 */
function expansionPlanOf(target: ExpandScrollTarget): ScrollPlan | null {
  const { card, body, collapsing, container: declared, bounds } = target
  const container = scrollContainerOf(card, declared)
  if (!container) return null
  const takeShrink = takeOnce(collapsing ? collapseShrinkAbove(collapsing, card) : 0)
  const measure = (): ScrollPlan => {
    const view = viewportOf(container)
    const cardRect = card.getBoundingClientRect()
    const insets = resolveScrollInsets(container, bounds)
    const targetTop = expansionTargetScrollTop({
      // 对齐卡片外框（border box），不再从标题行算起：上边框与圆角必须留在视口里。
      top: cardRect.top - view.top,
      height: expandedCardHeight(card, body),
      cardHeight: cardRect.height,
      shrink: takeShrink(),
      viewportHeight: view.viewportHeight,
      insetTop: insets.top,
      insetBottom: insets.bottom,
      scrollTop: view.scrollTop,
      scrollMax: view.scrollMax,
    })
    return { container, target: targetTop, scrollMax: view.scrollMax, recompute: measure }
  }
  return measure()
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
  const { card, body, container: declared, bounds } = target
  const container = scrollContainerOf(card, declared)
  const view = container ? viewportOf(container) : null
  const rect = view ? card.getBoundingClientRect() : null
  const detailHeight = body ? naturalContentHeight(body) : 0
  const top = rect ? rect.top - view!.top : 0
  const heightAfter = Math.max(0, (rect?.height ?? 0) - detailHeight)
  // 收起是居中，底边默认不额外留白；只有调用方显式声明底部边界时才让出它。
  const insets = container ? resolveScrollInsets(container, bounds, { bottom: 0 }) : { top: 0, bottom: 0 }
  const scrollMaxAfter = view ? Math.max(0, view.scrollMax - detailHeight) : 0
  collapse()
  if (!container || !view) return
  const { delta } = collapseScrollDelta({
    top,
    heightAfter,
    viewportHeight: view.viewportHeight,
    insetTop: insets.top,
    insetBottom: insets.bottom,
    scrollTop: view.scrollTop,
    scrollMaxAfter,
  })
  if (delta === 0) return
  /*
   * 折叠后按当时的几何重新居中：内容只会变短，对账一般不会被触发；可一旦容器
   * 因为别的动作变高（例如收起时另一张卡同时展开），补滚仍按同一套规则来。
   */
  const compute = (): ScrollPlan => {
    const after = viewportOf(container)
    const afterRect = card.getBoundingClientRect()
    const again = collapseScrollDelta({
      top: afterRect.top - after.top,
      heightAfter: afterRect.height,
      viewportHeight: after.viewportHeight,
      insetTop: insets.top,
      insetBottom: insets.bottom,
      scrollTop: after.scrollTop,
      scrollMaxAfter: scrollMaxOf(container),
    })
    return {
      container,
      target: after.scrollTop + again.delta,
      scrollMax: scrollMaxOf(container),
      recompute: compute,
    }
  }
  scrollAndSettle({
    container,
    target: view.scrollTop + delta,
    scrollMax: scrollMaxAfter,
    recompute: compute,
  }, target.settle !== false)
}

/** 正在跑的滚动：新的一次滚动要先把它停掉，避免两段动画抢同一个 scrollTop。 */
let activeScroll: ScrollRun | null = null

/** 等「内容长到目标位置可滚」的最长时间（毫秒）：与详情展开动画同量级，多留一点余量。 */
const REACHABLE_TIMEOUT = 420

/** 一次滚动的句柄：落位对账要看它最后写下的位置，新滚动要能把它停掉。 */
interface ScrollRun {
  /** 这次滚动最后写下的 `scrollTop`。 */
  lastSet(): number
  /** 被后来的一次滚动取消时为真：对账不再补滚。 */
  readonly cancelled: boolean
  /** 落定（动画跑完 / 跳到目标 / 被取消）时 resolve。 */
  readonly done: Promise<void>
  /** 取消这次滚动并落定。 */
  cancel(): void
}

/** 落位对账的一次计划：目标、计算它时用的可滚动上限、容器变高后的重算。 */
interface ScrollPlan {
  container: HTMLElement
  target: number
  scrollMax: number
  recompute: () => ScrollPlan
}

function easeInOutCubic(progress: number): number {
  return progress < 0.5
    ? 4 * progress * progress * progress
    : 1 - ((-2 * progress + 2) ** 3) / 2
}

function scrollMaxOf(container: HTMLElement): number {
  return Math.max(0, container.scrollHeight - container.clientHeight)
}

/**
 * 滚到目标。
 *
 * 减少动效的用户直接落位，但目标暂时滚不到（内容还在长高）时要等它够长：那一刻
 * 赋值会被浏览器夹回旧底，卡片底边永远贴不到视口底边。
 *
 * 动画模式下每帧把目标夹到「此刻」的可滚动范围，滚动就能跟着内容一起长出来
 * （原生 `scrollTo({ behavior: 'smooth' })` 在调用瞬间就把目标夹死，实测差出两百多像素）。
 * 用户自己滚了就取消，不再抢滚动条。
 */
function applyScroll(container: HTMLElement, target: number): ScrollRun {
  activeScroll?.cancel()
  activeScroll = null

  let cancelled = false
  let lastSet = container.scrollTop
  let frame = 0
  let run: ScrollRun | null = null
  let finish!: () => void
  const done = new Promise<void>((resolve) => { finish = resolve })

  /** 收尾：停掉等待中的帧、从全局登记里摘掉、让 `done` 落定。 */
  const stop = () => {
    if (frame) cancelAnimationFrame(frame)
    frame = 0
    if (activeScroll === run) activeScroll = null
    finish()
  }

  /** 立即落位；目标暂时不可达时等它够长，最多等 `REACHABLE_TIMEOUT`。 */
  const land = () => {
    const startedAt = performance.now()
    const step = () => {
      const max = scrollMaxOf(container)
      if (max >= target - 0.5 || performance.now() - startedAt > REACHABLE_TIMEOUT) {
        container.scrollTop = Math.min(Math.max(target, 0), max)
        lastSet = container.scrollTop
        stop()
        return
      }
      frame = requestAnimationFrame(step)
    }
    step()
  }

  run = {
    lastSet: () => lastSet,
    get cancelled() { return cancelled },
    done,
    cancel: () => {
      if (cancelled) return
      cancelled = true
      stop()
    },
  }

  if (prefersReducedMotion() || typeof requestAnimationFrame !== 'function') {
    activeScroll = run
    land()
    return run
  }

  const start = container.scrollTop
  const distance = target - start
  if (Math.abs(distance) < 1) {
    finish()
    return run
  }
  const startedAt = performance.now()

  const step = (now: number) => {
    const current = container.scrollTop
    const reachable = scrollMaxOf(container)
    /*
     * 用户自己滚了（滚轮 / 触摸 / 滚动条）：让位，不再抢 scrollTop。
     * 但内容变矮时浏览器会把 scrollTop 夹到新的上限，那也表现为
     * 「位置被改了」—— 这一种必须继续跑：展开另一张卡时上面
     * 那张同时收起，补偿滚动会被自己取消，卡片就被拽上去。
     */
    const clampedByContent = current < lastSet && lastSet > reachable
    if (!clampedByContent && Math.abs(current - lastSet) > USER_SCROLL_TOLERANCE) {
      stop()
      return
    }
    const progress = Math.min(1, (now - startedAt) / SCROLL_DURATION)
    const wanted = start + distance * easeInOutCubic(progress)
    // 每帧夹到「此刻」的可滚动范围：展开动画还没结束时范围会比目标小
    container.scrollTop = Math.min(wanted, reachable)
    lastSet = container.scrollTop
    if (progress < 1) {
      frame = requestAnimationFrame(step)
      return
    }
    // 动画结束时范围应该已经稳定；还差一点就补上，避免停在一两像素外
    container.scrollTop = Math.min(Math.max(target, 0), scrollMaxOf(container))
    lastSet = container.scrollTop
    stop()
  }

  activeScroll = run
  frame = requestAnimationFrame(step)
  return run
}

/**
 * 滚到计划位置，落位后按 `needsScrollSettle` 对账补滚。
 *
 * 对账看的是这次滚动**最后写下的位置**而不是目标：动画被用户抢走时位置对不上，
 * 直接让位；内容确实又长高了就重算目标再滚一次，最多 `SCROLL_SETTLE_ROUNDS` 次。
 */
function scrollAndSettle(plan: ScrollPlan, settle: boolean): void {
  let current = plan
  let rounds = 0
  const step = () => {
    const run = applyScroll(current.container, current.target)
    if (!settle) return
    void run.done.then(() => {
      if (run.cancelled) return
      if (!needsScrollSettle({
        scrollTop: current.container.scrollTop,
        lastSet: run.lastSet(),
        scrollMaxNow: scrollMaxOf(current.container),
        scrollMaxUsed: current.scrollMax,
        rounds,
      })) return
      rounds += 1
      current = current.recompute()
      step()
    })
  }
  step()
}
