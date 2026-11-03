/**
 * 教程覆盖层的几何计算。
 *
 * 纯函数、不碰 Vue 与 DOM（`tests/tutorial-geometry.test.mjs` 直接引用），
 * 这样「洞开在哪、卡片放哪、贴边怎么收」可以被单测钉住 —— 这几条是教程
 * 里最容易出错也最难肉眼发现的部分（卡片压住高亮、洞跑出视口、视口太矮时
 * 卡片被切掉一半）。
 *
 * 单位一律是**视口 CSS 像素**：V2 的界面缩放是 WebView content zoom
 * （`setWebviewZoom`），不是 CSS `transform: scale()`，所以
 * `getBoundingClientRect()` 的结果可以直接拿来定位 `position: fixed` 的浮层，
 * 不需要像 V1 那样再除一次缩放倍数。
 */

/** 视口坐标系里的矩形（与 `getBoundingClientRect` 同口径）。 */
export interface Rect {
  left: number
  top: number
  width: number
  height: number
}

export interface Size {
  width: number
  height: number
}

/** 卡片相对高亮框的方位。`auto` 按「下 → 上 → 右 → 左 → 居中」退让。 */
export type TutorialPlacement = 'auto' | 'top' | 'bottom' | 'left' | 'right' | 'center'

/** 高亮框与卡片之间的空隙。 */
export const CARD_GAP = 14
/** 卡片与视口边缘的最小留白。 */
export const VIEWPORT_MARGIN = 12

/** 把矩形四周外扩 `padding`（高亮框要比目标本身大一圈）。 */
export function inflateRect(rect: Rect, padding: number): Rect {
  if (padding <= 0) return { ...rect }
  return {
    left: rect.left - padding,
    top: rect.top - padding,
    width: rect.width + padding * 2,
    height: rect.height + padding * 2,
  }
}

/** 多个矩形的最小外接矩形；空数组返回 null。 */
export function unionRects(rects: readonly Rect[]): Rect | null {
  if (!rects.length) return null
  let left = Number.POSITIVE_INFINITY
  let top = Number.POSITIVE_INFINITY
  let right = Number.NEGATIVE_INFINITY
  let bottom = Number.NEGATIVE_INFINITY
  for (const rect of rects) {
    left = Math.min(left, rect.left)
    top = Math.min(top, rect.top)
    right = Math.max(right, rect.left + rect.width)
    bottom = Math.max(bottom, rect.top + rect.height)
  }
  if (!Number.isFinite(left) || !Number.isFinite(top)) return null
  return {
    left,
    top,
    width: Math.max(0, right - left),
    height: Math.max(0, bottom - top),
  }
}

/**
 * 把高亮框收进视口。
 *
 * 目标比视口还大（整列卡片、整张详情抽屉）时不外扩到负数：宽的取满宽、
 * 位置贴 0。宁可洞比目标小一圈，也不要高亮框跑到视口外被切掉一条。
 */
export function clampRectToViewport(rect: Rect, viewport: Size, margin = 0): Rect {
  const width = Math.min(rect.width, Math.max(0, viewport.width - margin * 2))
  const height = Math.min(rect.height, Math.max(0, viewport.height - margin * 2))
  const left = Math.min(Math.max(rect.left, margin), Math.max(margin, viewport.width - margin - width))
  const top = Math.min(Math.max(rect.top, margin), Math.max(margin, viewport.height - margin - height))
  return { left, top, width, height }
}


function fitsVertically(free: number, cardHeight: number): boolean {
  return free >= cardHeight + CARD_GAP + VIEWPORT_MARGIN
}

function fitsHorizontally(free: number, cardWidth: number): boolean {
  return free >= cardWidth + CARD_GAP + VIEWPORT_MARGIN
}

/**
 * 卡片落点。
 *
 * `auto` 的优先序是「下 → 上 → 右 → 左」，最后退化成居中 —— 与常见引导库
 * 一致：卡片压在目标下方读起来最自然，目标贴着视口底部时翻到上方。
 * 四种方位都放不下（视口又小又窄）时选空间最大的一侧硬放，再夹回视口，
 * 卡片最多盖住目标一部分，但不会跑到屏幕外。
 */
export function placeCard(
  rect: Rect | null,
  card: Size,
  viewport: Size,
  placement: TutorialPlacement = 'auto',
): { left: number; top: number } {
  const margin = VIEWPORT_MARGIN
  const maxLeft = Math.max(margin, viewport.width - margin - card.width)
  const maxTop = Math.max(margin, viewport.height - margin - card.height)
  const clamp = (left: number, top: number) => ({
    left: Math.min(Math.max(left, margin), maxLeft),
    top: Math.min(Math.max(top, margin), maxTop),
  })

  if (!rect || placement === 'center') {
    return clamp((viewport.width - card.width) / 2, (viewport.height - card.height) / 2)
  }

  const centerX = rect.left + rect.width / 2
  const centerY = rect.top + rect.height / 2
  const free = {
    below: viewport.height - (rect.top + rect.height),
    above: rect.top,
    right: viewport.width - (rect.left + rect.width),
    left: rect.left,
  }

  const spots: Record<Exclude<TutorialPlacement, 'auto' | 'center'>, () => { left: number; top: number }> = {
    bottom: () => ({ left: centerX - card.width / 2, top: rect.top + rect.height + CARD_GAP }),
    top: () => ({ left: centerX - card.width / 2, top: rect.top - CARD_GAP - card.height }),
    right: () => ({ left: rect.left + rect.width + CARD_GAP, top: centerY - card.height / 2 }),
    left: () => ({ left: rect.left - CARD_GAP - card.width, top: centerY - card.height / 2 }),
  }

  if (placement !== 'auto') {
    const spot = spots[placement]
    return clamp(spot().left, spot().top)
  }

  const fits: Array<{ side: keyof typeof free; ok: boolean }> = [
    { side: 'below', ok: fitsVertically(free.below, card.height) },
    { side: 'above', ok: fitsVertically(free.above, card.height) },
    { side: 'right', ok: fitsHorizontally(free.right, card.width) },
    { side: 'left', ok: fitsHorizontally(free.left, card.width) },
  ]
  const order: Array<Exclude<TutorialPlacement, 'auto' | 'center'>> = ['bottom', 'top', 'right', 'left']
  for (let i = 0; i < order.length; i += 1) {
    if (!fits[i].ok) continue
    const spot = spots[order[i]]()
    return clamp(spot.left, spot.top)
  }

  // 四面都挤：挑最大的那块空间，按对应方位放，再夹回视口。
  const widest = fits.reduce((best, item) => (free[item.side] > free[best.side] ? item : best), fits[0])
  const sideBySpace: Record<keyof typeof free, Exclude<TutorialPlacement, 'auto' | 'center'>> = {
    below: 'bottom',
    above: 'top',
    right: 'right',
    left: 'left',
  }
  const spot = spots[sideBySpace[widest.side]]()
  return clamp(spot.left, spot.top)
}
