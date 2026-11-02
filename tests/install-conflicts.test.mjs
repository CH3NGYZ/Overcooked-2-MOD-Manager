import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

/**
 * 安装冲突的「先卸载、再安装」（`src/installConflicts.ts`）。
 *
 * 这里测的是顺序与中止语义，而不是后端卸载本身：MOD 与地图两条线共用同一份
 * 循环，任何一处把「顺序执行」写成 `Promise.all`、或在失败后继续调用后面的
 * 卸载，都会让用户看到「装到一半、旧内容东少一块西少一块」。
 *
 * 与 `install-detail.test.mjs` 一样注册解析钩子，直接跑真实源码。
 */
registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.startsWith('.') && !/\.[cm]?[jt]s$/.test(specifier)) {
      try {
        return nextResolve(`${specifier}.ts`, context)
      } catch {
        // 不是单文件模块，继续试目录索引
      }
      try {
        return nextResolve(`${specifier}/index.ts`, context)
      } catch {
        // 交给 Node 自己报错，错误里带原始 specifier 更好查
      }
    }
    return nextResolve(specifier, context)
  },
})

const { uninstallConflicts, uninstallMapConflict, uninstallModConflict } =
  await import('../src/installConflicts.ts')
const { api } = await import('../src/services/tauri.ts')

/** 一个冲突项（字段与后端 `InstallConflict` 一致）。 */
function conflict(handle, overrides = {}) {
  return {
    name: `旧版 ${handle}`,
    version: '0.1',
    handle,
    enabled: true,
    same_version: false,
    ...overrides,
  }
}

/** 临时替换一个 api 方法，返回 { calls, restore }。 */
function stubApi(method, result) {
  const calls = []
  const original = api[method]
  api[method] = async (...args) => {
    calls.push(args)
    return typeof result === 'function' ? result(...args) : result
  }
  return { calls, restore: () => { api[method] = original } }
}

test('顺序执行：一项卸完才卸下一项，顺序与传入一致', async () => {
  const order = []
  let active = 0
  const result = await uninstallConflicts(
    [conflict('a.dll'), conflict('b.dll'), conflict('c.dll')],
    async (item) => {
      active += 1
      assert.equal(active, 1, '同一时刻只能有一项在卸载，不能并发')
      order.push(item.handle)
      await Promise.resolve()
      active -= 1
    },
  )

  assert.deepEqual(result, { ok: true, remaining: [] })
  assert.deepEqual(order, ['a.dll', 'b.dll', 'c.dll'])
})

test('第一个失败立即返回：拿到 failed 与已翻译原因，后续不再调用', async () => {
  const called = []
  const result = await uninstallConflicts(
    [conflict('a.dll'), conflict('b.dll'), conflict('c.dll')],
    async (item) => {
      called.push(item.handle)
      if (item.handle === 'b.dll') throw new Error('game.notFound')
    },
  )

  assert.equal(result.ok, false)
  assert.equal(result.failed.handle, 'b.dll', 'failed 指向第一个失败项')
  assert.deepEqual(called, ['a.dll', 'b.dll'], '失败项之后的冲突不再尝试卸载')
  assert.deepEqual(result.remaining.map((item) => item.handle), ['b.dll', 'c.dll'],
    '只剩没卸掉的项：重试时不能把已卸载的再卸一遍')
  assert.ok(result.reason.length > 0)
  assert.notEqual(result.reason, 'game.notFound', '原因要经过语言表翻译，而不是裸 tag')
})

test('冲突卸载的 MOD 走整包语义：allowUnmanaged = true，不弹二级确认', async () => {
  const stub = stubApi('uninstallLocalMod', { ok: true, message: 'message.mods.deleted' })
  try {
    await uninstallModConflict(conflict('Old.dll'))
  } finally {
    stub.restore()
  }
  assert.deepEqual(stub.calls, [['Old.dll', true]])
})

test('冲突卸载的地图带上目录名与当前启用状态；后端 ok=false 视为失败', async () => {
  const stub = stubApi('deleteLevelSet', (dirName, enabled) =>
    dirName === 'disabled-set'
      ? { ok: false, message: 'game.notFound' }
      : { ok: true, message: 'message.removed' })
  try {
    await uninstallMapConflict(conflict('enabled-set', { handle: 'enabled-set', enabled: true }))
    await assert.rejects(
      uninstallMapConflict(conflict('disabled-set', { handle: 'disabled-set', enabled: false })),
      /game\.notFound/,
    )
  } finally {
    stub.restore()
  }
  assert.deepEqual(stub.calls, [['enabled-set', true], ['disabled-set', false]],
    '句柄与 enabled 原样交给 IPC，不能自己猜目录')
})
