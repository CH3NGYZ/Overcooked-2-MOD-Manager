import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 下载安装面板的高度分配与滚轮接力（`src/releasePanelFit.ts`）。
 *
 * 口径来自爸爸 2026-11-02 的原话：日志区要保证最低高度（不能把日志显示区域
 * 挤没），同时「资产 / 安装」一行必须贴底、留在视口内；页面已经滚到顶还继续
 * 滚滚轮时，应该是上一层的页面往上滚。
 *
 * 这里锁定两条容易回归的算法：
 * 1. 面板高度怎么从抽屉可视高度取整、什么时候判定「矮到放不下」；
 * 2. `wheelNeedsHandoff` 只在「内层与抽屉都到边界、背后页面还能滚」时才接管。
 */
const { releasePanelHeight, fitsMinLogHeight, canScrollInDirection, wheelNeedsHandoff, RELEASE_LOG_MIN_HEIGHT } =
  await import('../src/releasePanelFit.ts')

/** 实测（2026-11-02，1280×720 窗口 + 150% 界面缩放）：抽屉可视 637，内容层上下内边距各 16。 */
const VIEWPORT = 605

const box = (scrollTop, clientHeight, scrollHeight) => ({ scrollTop, clientHeight, scrollHeight })

test('面板高度：向下取整，多出来的小数会让抽屉 body 冒出 1px 滚动条', () => {
  assert.equal(releasePanelHeight(VIEWPORT), VIEWPORT)
  assert.equal(releasePanelHeight(605.7), 605)
  assert.equal(releasePanelHeight(0.4), 0)
})

test('面板高度：负值（容器还没布局出来）按 0 处理，不写负数高度', () => {
  assert.equal(releasePanelHeight(-12), 0)
})

test('日志下限：够不够最低高度按 1px 容差判定（浏览器取整会让实测值差半像素）', () => {
  assert.equal(fitsMinLogHeight(RELEASE_LOG_MIN_HEIGHT), true)
  assert.equal(fitsMinLogHeight(RELEASE_LOG_MIN_HEIGHT - 1), true)
  assert.equal(fitsMinLogHeight(RELEASE_LOG_MIN_HEIGHT - 2), false, '压下最低高度就应该回退成不锁面板高度')
  assert.equal(fitsMinLogHeight(288), true)
  assert.equal(fitsMinLogHeight(0), false)
})

test('canScrollInDirection：上下各留 1px 容差，零位移不算可滚', () => {
  assert.equal(canScrollInDirection(box(0, 100, 300), 1), true)
  assert.equal(canScrollInDirection(box(0, 100, 300), -1), false)
  assert.equal(canScrollInDirection(box(200, 100, 300), 1), false)
  assert.equal(canScrollInDirection(box(200, 100, 300), -1), true)
  assert.equal(canScrollInDirection(box(0, 100, 100), 1), false)
  assert.equal(canScrollInDirection(box(0, 100, 100), 0), false)
  assert.equal(canScrollInDirection(box(0.6, 100, 300), -1), false, '不足 1px 视为到顶')
})

test('滚轮接力：内层还能滚时不接管', () => {
  const handoff = wheelNeedsHandoff({
    deltaY: 40,
    inner: box(0, 300, 900),
    outer: box(0, 600, 600),
    page: box(0, 600, 4000),
  })
  assert.equal(handoff, false)
})

test('滚轮接力：内层到边界、抽屉还能滚时交回浏览器原生滚动', () => {
  const handoff = wheelNeedsHandoff({
    deltaY: 40,
    inner: box(600, 300, 900),
    outer: box(0, 600, 1200),
    page: box(0, 600, 4000),
  })
  assert.equal(handoff, false)
})

test('滚轮接力：内层与抽屉都到边界、背后页面还能滚时接管', () => {
  assert.equal(
    wheelNeedsHandoff({
      deltaY: 40,
      inner: box(600, 300, 900),
      outer: box(0, 600, 600),
      page: box(0, 600, 4000),
    }),
    true,
  )
  assert.equal(
    wheelNeedsHandoff({
      deltaY: -40,
      inner: box(0, 300, 900),
      outer: box(0, 600, 600),
      page: box(1200, 600, 4000),
    }),
    true,
  )
})

test('滚轮接力：鼠标不在日志框里时只看抽屉与页面；页面也到头就不接管', () => {
  assert.equal(
    wheelNeedsHandoff({
      deltaY: -40,
      inner: null,
      outer: box(0, 600, 600),
      page: box(800, 600, 4000),
    }),
    true,
  )
  assert.equal(
    wheelNeedsHandoff({
      deltaY: -40,
      inner: null,
      outer: box(0, 600, 600),
      page: box(0, 600, 4000),
    }),
    false,
  )
})
