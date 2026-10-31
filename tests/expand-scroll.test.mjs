import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 卡片详情展开 / 收起的滚动策略（`src/expandScroll.ts`）。
 *
 * 这几条规则是爸爸口头提的交互细节，全靠几何算，最容易在「贴底展开最后一张卡」、
 * 「卡片外框贴页头」和「收起后居中」这三个边界上算错，所以逐条锁住。
 *
 * 口径：
 * - 展开用**卡片外框**（border box）的数字，不再是「标题行到卡片底边」；
 * - 顶部边界是 `insetTop` 而不是 0（页头在滚动区上方占掉的那一截）；
 * - 收起把折叠后的卡片外框滚到可视区正中（上下邻居都留着，方便连着点开）。
 */
const { expansionScrollDelta, collapseScrollDelta, EXPANSION_BOTTOM_GAP } = await import('../src/expandScroll.ts')

/** 视口 626 高：实测窗口里 `.page-scroll .n-scrollbar-container` 的可视高度。 */
const VIEWPORT = 626
/** 可用底边：视口底边再往上让出 12px，卡片底边对齐到这里而不是贴着视口。 */
const USABLE_BOTTOM = VIEWPORT - 12
/** 折叠后的卡片外框高度：实测一张 MOD 卡片的标题行 + 上下内边距 + 边框约 44px。 */
const COLLAPSED_H = 44
/** 居中时卡片顶边的位置（insetTop = 0）：(626 - 44) / 2。 */
const MID_TOP = (VIEWPORT - COLLAPSED_H) / 2

function expand(overrides = {}) {
  return expansionScrollDelta({
    top: 0,
    height: 0,
    viewportHeight: VIEWPORT,
    scrollTop: 0,
    scrollMax: 1000,
    ...overrides,
  })
}

function collapse(overrides = {}) {
  return collapseScrollDelta({
    top: 400,
    heightAfter: COLLAPSED_H,
    viewportHeight: VIEWPORT,
    scrollTop: 120,
    scrollMaxAfter: 2000,
    ...overrides,
  })
}

test('底部余量是 12px，可调', () => {
  assert.equal(EXPANSION_BOTTOM_GAP, 12)
})

test('展开：原地能完整看见就不滚', () => {
  const { delta, reason } = expand({ top: 100, height: 300 })
  assert.equal(delta, 0)
  assert.equal(reason, 'none')
})

test('展开：底边正好落在可用底边上算看得见，多 1px 才滚', () => {
  assert.equal(expand({ top: USABLE_BOTTOM - 300, height: 300 }).delta, 0)
  assert.equal(expand({ top: USABLE_BOTTOM - 300, height: 300 }).reason, 'none')
  assert.equal(expand({ top: USABLE_BOTTOM - 300 + 1, height: 300 }).delta, 1)
})

test('展开：装不下就上移，卡片底边停在视口底边上方 12px', () => {
  const { delta, reason } = expand({ top: 400, height: 300 })
  assert.equal(delta, 400 + 300 - USABLE_BOTTOM)
  assert.equal(reason, 'reveal-bottom')
})

test('展开：上移会让卡片外框顶出边界时，改成外框贴边界、其余向下溢出', () => {
  const { delta, reason } = expand({ top: 200, height: 800 })
  // 底边对齐需要 386，但会把外框顶到 -186；退化成「外框贴边界」= 上移 200
  assert.equal(delta, 200)
  assert.equal(reason, 'pin-top')
})

test('展开：底边对齐距离与贴边界距离相等时不算越界', () => {
  // top + height - 可用底边 === top - insetTop  =>  height === 可用底边
  const { delta, reason } = expand({ top: 120, height: USABLE_BOTTOM })
  assert.equal(delta, 120)
  assert.equal(reason, 'reveal-bottom')
})

test('展开：顶部边界用 insetTop —— 卡片外框贴的是页头下沿，不是 0', () => {
  // insetTop 64：最多只能上移 236（300 - 64），底边对齐要 386，所以被夹到 236
  const { delta, reason } = expand({ top: 300, height: 700, insetTop: 64 })
  assert.equal(delta, 236)
  assert.equal(reason, 'pin-top')
  // 底边对齐距离没超过这个上限时，仍然按底边对齐
  const fits = expand({ top: 200, height: 500, insetTop: 64 })
  assert.equal(fits.delta, 200 + 500 - USABLE_BOTTOM)
  assert.equal(fits.reason, 'reveal-bottom')
})

test('展开：卡片外框已经在页头之上（用户自己滚下去了）就不再往前推', () => {
  const { delta, reason } = expand({ top: 50, height: 900, insetTop: 64 })
  assert.equal(delta, 0)
  assert.equal(reason, 'pin-top')
})

test('展开：详情比视口还高时，外框贴边界、下面溢出', () => {
  // 外框此刻就在边界上：底边对齐会把外框顶出界，夹到 0（外框不动、详情向下溢出）
  const pinned = expand({ top: 0, height: 1200 })
  assert.equal(pinned.delta, 0)
  assert.equal(pinned.reason, 'pin-top')
  const below = expand({ top: 300, height: 1200 })
  assert.equal(below.delta, 300)
  assert.equal(below.reason, 'pin-top')
})

test('展开：吸顶中的卡片（外框远在页头之上）不滚', () => {
  assert.equal(expand({ top: -420, height: 500 }).delta, 0)
})

test('展开：可滚动范围不够时夹到容器底', () => {
  assert.equal(expand({ top: 400, height: 300, scrollMax: 50 }).delta, 50)
})

test('展开：已经在容器最底部时不再往下滚', () => {
  assert.equal(expand({ top: 500, height: 400, scrollTop: 200, scrollMax: 200 }).delta, 0)
})

test('展开：底边余量可被调用方覆盖', () => {
  assert.equal(expand({ top: 326, height: 300, bottomGap: 0 }).delta, 0)
  assert.equal(expand({ top: 326, height: 300 }).delta, 12)
})

test('收起：把折叠后的卡片滚到可视区正中', () => {
  // 居中位置：卡片顶边落在 (626 - 44) / 2 = 291，卡片中心正好 313 = 视口中心
  const { delta, reason } = collapse()
  assert.equal(delta, 400 - MID_TOP)
  assert.equal(reason, 'center')
  assert.equal(400 - delta + COLLAPSED_H / 2, VIEWPORT / 2)
})

test('收起：卡片在顶部附近时向上滚，在底部附近时向下滚', () => {
  // 顶部附近：卡片要往下走 → 往上滚（scrollTop 留够空间，别被 0 夹住）
  assert.ok(collapse({ top: 60, scrollTop: 400 }).delta < 0)
  assert.equal(collapse({ top: 60, scrollTop: 400 }).delta, 60 - MID_TOP)
  // 底部附近：卡片要往上走 → 往下滚
  assert.ok(collapse({ top: 560 }).delta > 0)
  assert.equal(collapse({ top: 560 }).delta, 560 - MID_TOP)
})

test('收起：已经居中（±2px）就不滚', () => {
  assert.equal(collapse({ top: MID_TOP }).delta, 0)
  assert.equal(collapse({ top: MID_TOP }).reason, 'none')
  assert.equal(collapse({ top: MID_TOP + 2 }).delta, 0)
  assert.equal(collapse({ top: MID_TOP - 2 }).delta, 0)
  // 超出容差就滚
  assert.equal(collapse({ top: MID_TOP + 2.5 }).reason, 'center')
})

test('收起：居中位置按「页头下沿到视口底边」算，不是整个视口', () => {
  // insetTop 64：居中顶边 = 64 + (626 - 64 - 44) / 2 = 323
  assert.equal(collapse({ top: 600, insetTop: 64 }).delta, 600 - 323)
  // 已经居中就不动
  assert.equal(collapse({ top: 323, insetTop: 64 }).delta, 0)
})

test('收起：卡片比可视区还高也照公式居中，不退化成贴顶', () => {
  // 居中顶边 = (626 - 800) / 2 = -87（在页头之上，说明卡片中心仍在可视区中心）
  const { delta, reason } = collapse({ top: 200, heightAfter: 800 })
  assert.equal(reason, 'center')
  assert.equal(delta, 200 - (VIEWPORT - 800) / 2)
})

test('收起：目标夹进收起后的可滚动范围（短页面尽量居中）', () => {
  // 目标 120 + 109 = 229，但收起后最高只能到 150
  const clamped = collapse({ scrollTop: 120, scrollMaxAfter: 150 })
  assert.equal(clamped.delta, 150 - 120)
  assert.equal(clamped.reason, 'center')
  // 目标为负时夹到 0
  assert.equal(collapse({ top: 60, scrollTop: 100, scrollMaxAfter: 900 }).delta, -100)
  // 收起后完全不可滚动
  assert.equal(collapse({ scrollTop: 0, scrollMaxAfter: 0 }).delta, 0)
})

test('收起：短页面上夹紧后卡片外框不会跑到页头之上', () => {
  // 已经滚到顶（scrollTop 0）但目标还要往上：夹到 0，卡片外框停在原位（top > insetTop）
  const frame = { top: 100, scrollTop: 0, scrollMaxAfter: 400 }
  const { delta } = collapse(frame)
  assert.equal(delta, 0)
  assert.ok(frame.top + delta >= 0)
})
