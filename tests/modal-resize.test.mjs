import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 弹窗尺寸变化的过渡判定（`src/modalResize.ts`）。
 *
 * 口径来自爸爸 2026-11-02 的原话：内部元素宽/高突然变化时，modal 自己也突然
 * 改变尺寸，希望平滑过渡。这里锁住「什么时候该补这段过渡」，因为多补一次会
 * 和进场动画打架、和用户拖窗口打架，少补一次就是这次的 bug 回归。
 */
const {
  shouldAnimateModalResize,
  modalResizeKeyframes,
  MODAL_RESIZE_DURATION,
  MODAL_RESIZE_EASING,
  MODAL_RESIZE_MIN_DELTA,
} = await import('../src/modalResize.ts')

const size = (width, height) => ({ width, height })
const base = { from: size(520, 300), to: size(520, 380), animating: false }

test('高度变了就补过渡（切 tab / 展开详情最典型）', () => {
  assert.equal(shouldAnimateModalResize(base), true)
})

test('宽度变了同样补过渡', () => {
  assert.equal(shouldAnimateModalResize({ ...base, from: size(480, 300), to: size(640, 300) }), true)
})

test('宽高都变、两个方向对称判定', () => {
  assert.equal(shouldAnimateModalResize({ ...base, from: size(640, 380), to: size(480, 300) }), true)
})

test('小于阈值的取整抖动不补过渡（浏览器 0.5px 取整会让每帧都触发）', () => {
  assert.equal(
    shouldAnimateModalResize({ ...base, to: size(520, 300 + MODAL_RESIZE_MIN_DELTA - 1) }),
    false,
  )
})

test('正好达到阈值才算变化', () => {
  assert.equal(
    shouldAnimateModalResize({ ...base, to: size(520, 300 + MODAL_RESIZE_MIN_DELTA) }),
    true,
  )
})

test('上一段过渡还在播时不叠加（否则两段动画抢同一个 width/height）', () => {
  assert.equal(shouldAnimateModalResize({ ...base, animating: true }), false)
})

test('视口尺寸变了不补过渡：那是用户自己拖窗口 / 切界面缩放', () => {
  assert.equal(shouldAnimateModalResize({ ...base, viewportChanged: true }), false)
})

test('系统「减少动态效果」时不补过渡', () => {
  assert.equal(shouldAnimateModalResize({ ...base, reducedMotion: true }), false)
})

test('刚挂载（0 尺寸）不补过渡：那是进场动画的范畴', () => {
  assert.equal(shouldAnimateModalResize({ ...base, from: size(0, 0) }), false)
  assert.equal(shouldAnimateModalResize({ ...base, from: size(520, 0) }), false)
  assert.equal(shouldAnimateModalResize({ ...base, to: size(0, 0) }), false)
})

test('关键帧：两个端点，都是 px 字符串，首帧是旧尺寸、末帧是新尺寸', () => {
  const frames = modalResizeKeyframes(size(520, 300), size(520, 380))
  assert.deepEqual(frames, [
    { width: '520px', height: '300px' },
    { width: '520px', height: '380px' },
  ])
})

test('时长与缓动是单一来源，组件不另写一份', () => {
  assert.equal(MODAL_RESIZE_DURATION, 200)
  assert.equal(MODAL_RESIZE_EASING, 'cubic-bezier(.4, 0, .2, 1)')
})
