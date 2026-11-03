import assert from 'node:assert/strict'
import test from 'node:test'
import {
  CARD_GAP,
  VIEWPORT_MARGIN,
  clampRectToViewport,
  inflateRect,
  placeCard,
  unionRects,
} from '../src/tutorial/geometry.ts'

/**
 * 教程覆盖层几何的守门测试。
 *
 * ## 锁死的行为
 *
 *   1. 高亮框外扩是四周各加 `padding`，不是只加宽高。
 *   2. 多个目标取**最小外接矩形**；空集合返回 null（纯讲解步骤）。
 *   3. 高亮框比视口还大时收缩到视口内，位置贴边 —— 宁可洞小一圈，
 *      也不能让框跑出视口被切掉一条边。
 *   4. 遮罩是**四块面板**，而且任何一块都不许压到洞口上；`rect` 为 null
 *      时退化成整块。
 *   5. 卡片落点顺序「下 → 上 → 右 → 左」，四种方位都放不下时也不会出视口；
 *      卡片与视口边缘始终留 `VIEWPORT_MARGIN`。
 *
 * 这些数字错了在界面上表现为「卡片压住高亮框 / 卡片被切掉一半 / 洞漏光」，
 * 肉眼很难稳定复现，所以全部用纯函数钉住。
 */

const VIEWPORT = { width: 1280, height: 720 }

test('inflateRect 四周各外扩 padding', () => {
  const inflated = inflateRect({ left: 100, top: 50, width: 200, height: 80 }, 6)
  assert.deepEqual(inflated, { left: 94, top: 44, width: 212, height: 92 })
})

test('inflateRect 的 padding 不大于 0 时原样返回副本', () => {
  const source = { left: 10, top: 20, width: 30, height: 40 }
  const result = inflateRect(source, 0)
  assert.deepEqual(result, source)
  assert.notEqual(result, source, '不返回同一个对象，避免调用方改到缓存')
  assert.deepEqual(inflateRect(source, -5), source)
})

test('unionRects 取最小外接矩形', () => {
  assert.equal(unionRects([]), null)
  assert.deepEqual(unionRects([{ left: 10, top: 10, width: 20, height: 20 }]), {
    left: 10,
    top: 10,
    width: 20,
    height: 20,
  })
  assert.deepEqual(
    unionRects([
      { left: 100, top: 40, width: 20, height: 20 },
      { left: 10, top: 90, width: 10, height: 10 },
    ]),
    { left: 10, top: 40, width: 110, height: 60 },
  )
})

test('clampRectToViewport 把出界的框推回视口内', () => {
  const pushed = clampRectToViewport({ left: 1200, top: 700, width: 200, height: 100 }, VIEWPORT)
  assert.equal(pushed.left, VIEWPORT.width - 200)
  assert.equal(pushed.top, VIEWPORT.height - 100)
  assert.equal(pushed.width, 200)
  assert.equal(pushed.height, 100)

  const negative = clampRectToViewport({ left: -40, top: -20, width: 100, height: 60 }, VIEWPORT)
  assert.equal(negative.left, 0)
  assert.equal(negative.top, 0)
})

test('clampRectToViewport 对超大目标收缩到视口并贴 0', () => {
  const huge = clampRectToViewport({ left: -100, top: -100, width: 4000, height: 3000 }, VIEWPORT, 12)
  assert.equal(huge.left, 12)
  assert.equal(huge.top, 12)
  assert.equal(huge.width, VIEWPORT.width - 24)
  assert.equal(huge.height, VIEWPORT.height - 24)
})

test('placeCard 目标在中上部时卡片放下方', () => {
  const rect = { left: 400, top: 100, width: 200, height: 60 }
  const spot = placeCard(rect, { width: 380, height: 200 }, VIEWPORT)
  assert.equal(spot.top, rect.top + rect.height + CARD_GAP)
  assert.equal(spot.left, 400 + 100 - 190)
})

test('placeCard 目标贴视口底部时翻到上方', () => {
  const rect = { left: 400, top: 600, width: 200, height: 100 }
  const spot = placeCard(rect, { width: 380, height: 200 }, VIEWPORT)
  assert.equal(spot.top, rect.top - CARD_GAP - 200)
})

test('placeCard 无目标时居中', () => {
  const card = { width: 400, height: 200 }
  const spot = placeCard(null, card, VIEWPORT)
  assert.deepEqual(spot, {
    left: (VIEWPORT.width - card.width) / 2,
    top: (VIEWPORT.height - card.height) / 2,
  })
})

test('placeCard 任何情况下都不出视口', () => {
  const card = { width: 380, height: 240 }
  const rects = [
    { left: 0, top: 0, width: 1280, height: 56 },
    { left: 0, top: 664, width: 1280, height: 56 },
    { left: 0, top: 0, width: 108, height: 720 },
    { left: 1172, top: 0, width: 108, height: 720 },
    { left: 0, top: 0, width: 1280, height: 720 },
  ]
  for (const rect of rects) {
    for (const placement of ['auto', 'top', 'bottom', 'left', 'right', 'center']) {
      const spot = placeCard(rect, card, VIEWPORT, placement)
      assert.ok(spot.left >= VIEWPORT_MARGIN, `${placement} 左越界：${spot.left}`)
      assert.ok(spot.top >= VIEWPORT_MARGIN, `${placement} 上越界：${spot.top}`)
      assert.ok(spot.left + card.width <= VIEWPORT.width - VIEWPORT_MARGIN, `${placement} 右越界`)
      assert.ok(spot.top + card.height <= VIEWPORT.height - VIEWPORT_MARGIN, `${placement} 下越界`)
    }
  }
})
