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
 * - 顶部边界是 `insetTop` 而不是 0（页头在滚动区上方占掉的那一截），底部留白可被
 *   `insetBottom` 覆盖（`bottomGap` 是旧名，只为兼容既有调用方）；
 * - 收起把折叠后的卡片外框滚到可视区正中（底边默认不留白，显式给了才让出）；
 * - 落位后按 `needsScrollSettle` 决定要不要补滚（弹窗滚动途中内容还在长高）。
 */
const {
  expansionScrollDelta,
  expansionTargetScrollTop,
  collapseScrollDelta,
  resolveScrollInsets,
  needsScrollSettle,
  takeOnce,
  SCROLL_SETTLE_ROUNDS,
  EXPANSION_BOTTOM_GAP,
} = await import('../src/expandScroll.ts')

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

/** 展开前的卡片高度：标题行 + 内边距 + 边框（与 COLLAPSED_H 同一个实测值）。 */
const CARD_H = COLLAPSED_H

function expandTarget(overrides = {}) {
  return expansionTargetScrollTop({
    top: 0,
    height: 0,
    cardHeight: CARD_H,
    viewportHeight: VIEWPORT,
    scrollTop: 0,
    scrollMax: 1000,
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

test('展开目标：没有被动收起的卡片时，目标就是 scrollTop + delta', () => {
  const frame = { top: 400, height: 300, scrollTop: 120, scrollMax: 2000 }
  assert.equal(expandTarget(frame), 120 + expand(frame).delta)
})

test('展开目标：上方卡片收起把内容带上去多少，滚动位置就减多少（标题留在原地）', () => {
  // 原地放得下（delta = 0），上方那张缩掉 380：只有把滚动位置也减掉 380，标题才不动。
  assert.equal(expandTarget({ top: 100, height: 300, scrollTop: 900, shrink: 380 }), 900 - 380)
})

test('展开目标：撞底时标题只上移露底需要的距离，不被上方收起拽走', () => {
  const top = 400
  const height = 300
  const scrollTop = 1000
  const target = expandTarget({ top, height, scrollTop, scrollMax: 4000, shrink: 380 })
  const reveal = top + height - USABLE_BOTTOM
  assert.equal(target, scrollTop + reveal - 380)
  // 标题最终位置 = top - reveal：只上移了露底那一段，没有被 380 全吃掉。
  assert.equal(top - reveal, USABLE_BOTTOM - height)
  assert.ok(top - reveal > 0)
})

test('展开目标：标题不会被上方收起挤到页头之上（下界夹到 0）', () => {
  assert.equal(expandTarget({ top: 400, height: 300, scrollTop: 200, shrink: 380 }), 0)
})

test('展开目标：卡片比视口高时标题贴住顶部边界，其余向下溢出', () => {
  const target = expandTarget({ top: 20, height: 900, scrollTop: 300, scrollMax: 4000, insetTop: 56 })
  assert.equal(target, 300 + (20 - 56 < 0 ? 0 : 20 - 56))
  assert.ok(target - 300 <= 20)
})

test('展开目标：夹进动画结束后的可滚动范围（上面缩掉、自己长高）', () => {
  // scrollMaxAfter = 200 + 300 - 44 - 380 -> 0，滚动位置被压到 76
  const target = expandTarget({ top: 400, height: 300, scrollTop: 900, scrollMax: 500, shrink: 380 })
  // scrollMaxAfter = 500 + 300 - 44 - 380 = 376
  assert.equal(target, 376)
})

test('展开目标：没有被动收起时与 delta 版本一致', () => {
  const frame = { top: 400, height: 300, scrollTop: 900, scrollMax: 1000 }
  const target = expandTarget(frame)
  // scrollMaxAfter = 1000 + 300 - 44 = 1256；露底需要 86px，上限足够
  assert.equal(target, 900 + 86)
  assert.equal(target, 900 + expand(frame).delta)
})

test('展开目标：卡片比视口还高时，标题贴到上界、其余向下溢出', () => {
  const insetTop = 56
  const target = expandTarget({ top: 400, height: 900, scrollTop: 300, scrollMax: 4000, insetTop })
  // 露底要 686px，但标题只能上移 400 - 56 = 344px（贴住上界）
  assert.equal(target, 300 + 344)
  assert.equal(400 - (target - 300), insetTop)
})

test('展开：底边余量走 insetBottom，旧名 bottomGap 仍然生效', () => {
  // 卡片 bottom 627：默认留 12px 要滚 13px；不留白只差 1px，留 40px 要滚 41px
  assert.equal(expand({ top: 327, height: 300 }).delta, 13)
  assert.equal(expand({ top: 327, height: 300, insetBottom: 0 }).delta, 1)
  assert.equal(expand({ top: 327, height: 300, insetBottom: 40 }).delta, 41)
  assert.equal(expand({ top: 327, height: 300, bottomGap: 0 }).delta, 1)
  // 两个都给时以正式名 insetBottom 为准
  assert.equal(expand({ top: 327, height: 300, insetBottom: 0, bottomGap: 40 }).delta, 1)
})

test('展开目标：底边余量同样走 insetBottom', () => {
  const frame = { top: 300, height: 300, scrollTop: 100, scrollMax: 2000 }
  // 默认留 12px：卡片 bottom 600 还在可用底边 614 之内，停在原地
  assert.equal(expandTarget(frame), 100)
  // 留 40px：可用底边 586，要把卡片往上推 14px
  assert.equal(expandTarget({ ...frame, insetBottom: 40 }), 114)
})

test('收起：底边余量默认 0（居中上下对称），显式给了才让出', () => {
  // 居中顶边 = (626 - 44) / 2 = 291
  assert.equal(collapse({ top: 600 }).delta, 600 - MID_TOP)
  // insetBottom 20：可用高度 606，居中顶边 = (626 - 20 - 44) / 2 = 281
  assert.equal(collapse({ top: 600, insetBottom: 20 }).delta, 600 - 281)
})

test('边界解析：数字按 px，元素按实测边，缺省回到页面规则', () => {
  const container = {
    getBoundingClientRect: () => ({ top: 100, bottom: 500 }),
    closest: () => null,
  }
  // 数字直接就是「距容器可视顶 / 距容器可视底」的 px
  assert.deepEqual(resolveScrollInsets(container, { top: 24, bottom: 16 }), { top: 24, bottom: 16 })
  // 顶部元素用自己的底边：页头下沿 160 - 容器顶 100 = 60
  assert.equal(resolveScrollInsets(container, { top: { getBoundingClientRect: () => ({ bottom: 160 }) } }).top, 60)
  // 底部元素用自己的顶边：容器底 500 - 工具条顶 460 = 40
  assert.equal(resolveScrollInsets(container, { bottom: { getBoundingClientRect: () => ({ top: 460 }) } }).bottom, 40)
  // 缺省：容器不在 .app-main 里时顶部 0，底部回到 12px 余量
  assert.deepEqual(resolveScrollInsets(container), { top: 0, bottom: EXPANSION_BOTTOM_GAP })
  // 弹窗：顶部写 0、底部不额外留白
  assert.deepEqual(resolveScrollInsets(container, { top: 0 }, { bottom: 0 }), { top: 0, bottom: 0 })
})

test('边界解析：落在容器之外按 0 算，负留白不会把卡片推出容器', () => {
  const container = {
    getBoundingClientRect: () => ({ top: 100, bottom: 500 }),
    closest: () => null,
  }
  assert.equal(resolveScrollInsets(container, { top: { getBoundingClientRect: () => ({ bottom: 80 }) } }).top, 0)
  assert.equal(resolveScrollInsets(container, { bottom: { getBoundingClientRect: () => ({ top: 520 }) } }).bottom, 0)
  assert.equal(resolveScrollInsets(container, { top: -20, bottom: -8 }).top, 0)
})

test('落位对账：内容变高、用户没滚过才补滚', () => {
  const base = { scrollTop: 900, lastSet: 900, scrollMaxNow: 600, scrollMaxUsed: 400, rounds: 0 }
  assert.equal(needsScrollSettle(base), true)
  // 容器没有比计算时更高：目标没被夹短，补滚没意义
  assert.equal(needsScrollSettle({ ...base, scrollMaxNow: 400 }), false)
  // 亚像素增长不算内容长高（scrollHeight 取整、clientHeight 可能带小数）
  assert.equal(needsScrollSettle({ ...base, scrollMaxNow: 400.5 }), false)
})

test('落位对账：用户自己滚过就让位', () => {
  const base = { scrollMaxNow: 600, scrollMaxUsed: 400, rounds: 0 }
  assert.equal(needsScrollSettle({ ...base, scrollTop: 905, lastSet: 900 }), false)
  // 差在容差之内仍算滚动动画自己写的
  assert.equal(needsScrollSettle({ ...base, scrollTop: 902, lastSet: 900 }), true)
})

test('落位对账：最多补滚 SCROLL_SETTLE_ROUNDS 次', () => {
  const frame = { scrollTop: 900, lastSet: 900, scrollMaxNow: 600, scrollMaxUsed: 400 }
  assert.equal(SCROLL_SETTLE_ROUNDS, 2)
  assert.equal(needsScrollSettle({ ...frame, rounds: 0 }), true)
  assert.equal(needsScrollSettle({ ...frame, rounds: 1 }), true)
  assert.equal(needsScrollSettle({ ...frame, rounds: 2 }), false)
  // 调用方也可以收紧上限
  assert.equal(needsScrollSettle({ ...frame, rounds: 1, maxRounds: 1 }), false)
})

/*
 * 收缩补偿是「改状态前量好的预估」，只在第一次计算里成立。
 *
 * 展开 B 会让上一张 A 收起：A 的详情高度在改状态那一刻量好、从目标滚动位置里减掉。
 * 滚动开始后 A 真的收下去了，卡片 top 与可滚动上限都自带这段位移；落位对账重算时
 * 再减一次，目标就落到当前滚动位置之上 —— 卡片反着滚回去（真机实测 −588px，
 * 约等于 A 的详情高度）。所以这个量只能取一次。
 */
test('一次性补偿：第一次取改状态前量好的收缩量，之后恒为 0', () => {
  const take = takeOnce(588)
  assert.equal(take(), 588, '第一次用改状态前量好的收缩量')
  assert.equal(take(), 0, '滚动开始后几何自带这段位移，再减就是补偿两遍')
  assert.equal(take(), 0)
})

test('一次性补偿：没有上一张卡收起时恒为 0，普通展开不受影响', () => {
  const take = takeOnce(0)
  assert.equal(take(), 0)
  assert.equal(take(), 0)
})
