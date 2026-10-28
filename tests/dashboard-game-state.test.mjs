import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 游戏状态事件的合并必须整体替换快照，不能就地改内部属性。
 *
 * 真实 bug：`applyGame` 里写的是 `dashboard.value.game = value`，
 * 而 `dashboard` 是 `shallowRef`。shallowRef 只追踪 `.value` 的引用替换，
 * 对内部属性的修改不产生任何依赖通知 —— 于是所有 computed
 * （`game.kitchen_running`、`frameworkOk` 等）永远拿到旧值。
 *
 * 表现就是：游戏从 Steam 手动启动后，管理器界面一直显示「未运行」，
 * 而后端日志里早就推送了 `游戏运行=true`。
 *
 * 这个测试用真实的 Vue 复现该语义，锁定正确写法。
 */
const { computed, effect, shallowRef, nextTick } = await import('vue')

function mockDashboard(running) {
  return {
    game: { kitchen_running: running, path: 'C:/game' },
    mods: [{ name: 'big list' }],
  }
}

test('shallowRef 就地改内部属性不会触发 computed 更新', () => {
  const dashboard = shallowRef(mockDashboard(false))
  const running = computed(() => dashboard.value.game.kitchen_running)

  assert.equal(running.value, false)

  // 这就是修复前的写法
  dashboard.value.game.kitchen_running = true

  // computed 没有失效，仍然是旧值 —— 这正是 bug 的本质
  assert.equal(running.value, false, 'shallowRef 内部属性的修改不应被期待为响应式')
})

test('shallowRef 整体替换快照会正确触发更新', () => {
  const dashboard = shallowRef(mockDashboard(false))
  const running = computed(() => dashboard.value.game.kitchen_running)
  const seen = []
  effect(() => seen.push(running.value))

  assert.equal(running.value, false)

  // 正确写法：整对象替换
  const current = dashboard.value
  dashboard.value = { ...current, game: { kitchen_running: true, path: 'C:/game' } }

  assert.equal(running.value, true)
  assert.deepEqual(seen, [false, true], '侦听方应收到两次值')
})

/*
 * 合并语义：只替换 game 字段，其余字段（mods 等）保持同一引用。
 * 这条约束让「游戏状态事件」不必复制几万条 MOD 记录。
 */
test('整体替换只换 game 字段，mods 引用保持不变', () => {
  const dashboard = shallowRef(mockDashboard(false))
  const modsBefore = dashboard.value.mods

  const current = dashboard.value
  dashboard.value = { ...current, game: { kitchen_running: true, path: 'C:/game' } }

  assert.equal(dashboard.value.mods, modsBefore, 'mods 数组不应被复制')
  assert.equal(dashboard.value.game.kitchen_running, true)
})

/* 未加载快照时收到事件不应崩溃，也不该凭空造一个 dashboard。 */
test('快照尚未加载时收到事件是安全的', () => {
  const dashboard = shallowRef(null)
  const current = dashboard.value
  if (current) dashboard.value = { ...current, game: { kitchen_running: true } }
  assert.equal(dashboard.value, null)
})
