import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'
import { registerHooks } from 'node:module'

/**
 * 冲突前置门控的接线（`useInstallCheck` + `useLevelSetImport.finishStaged`）。
 *
 * 这一条是需求的核心：**处理完冲突才问「要不要装」**。放在这里钉住的原因是
 * 它跨了三个文件（共享弹窗 composable、地图导入流程、两个宿主视图），任何一处
 * 漂了都不会有编译错误，只会在用户手上变成「确认过又被要求确认一次」或者
 * 「冲突没卸就导入，后端回收目录时打架」。
 *
 * 分两部分：
 *   · 运行时：`useInstallCheck` 的 Promise 语义（结论、取消、重开、执行器转发）；
 *   · 结构：`finishStaged` 里 resolveConflicts 必须早于 askApproval，旧的
 *     while 卸载循环必须已经删掉，两个视图必须都挂上新弹窗。
 *
 * `useLevelSetImport` 依赖 Vue 与 `services/tauri`，Node 里跑不了完整流程，
 * 所以那一半用源码结构断言（与 tests/install-confirm-order.test.mjs 同风格）。
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

const { useInstallCheck } = await import('../src/composables/useInstallCheck.ts')

/** 等 watch 的 flush（默认 pre，走微任务队列）落地。 */
const tick = () => new Promise((resolve) => setTimeout(resolve, 0))

const model = (rows = []) => ({
  title: '安装前检查',
  lead: '说明',
  rows,
  blocked: [],
  sameVersionLabel: '同版本',
})

const row = (key = 'uninstall-a') => ({
  key,
  kind: 'uninstall',
  action: 'uninstall',
  actionLabel: '卸载',
  label: key,
  index: 0,
})

/* ===== 运行时：Promise 结论 ===== */

test('run 打开弹窗，finish 给出结论', async () => {
  const check = useInstallCheck()
  assert.equal(check.open.value, false)

  const pending = check.run(model(), async () => undefined)
  assert.equal(check.open.value, true, 'run 必须自己打开弹窗')
  assert.equal(check.model.value?.title, '安装前检查')

  check.finish(true)
  assert.equal(await pending, true)
})

test('弹窗被收起但没给结论，算取消', async () => {
  const check = useInstallCheck()
  const pending = check.run(model(), async () => undefined)
  // 模拟 v-model 把 show 改成 false（Esc / 叉掉 / 宿主自己关）
  check.open.value = false
  await tick()
  assert.equal(await pending, false, '没有结论就关闭必须 resolve(false)，否则流程永远挂着')
})

test('上一轮没结论就开新窗，上一轮按放弃处理', async () => {
  const check = useInstallCheck()
  const first = check.run(model(), async () => undefined)
  const second = check.run(model(), async () => undefined)

  assert.equal(await first, false, '被顶掉的那一轮必须立刻结束，不能等第二次结论')
  check.finish(true)
  assert.equal(await second, true, '第二次的结论不能被上一轮吃掉')
})

test('execute 转发给本次注入的执行器', async () => {
  const check = useInstallCheck()
  const calls = []
  const pending = check.run(model(), async (item) => { calls.push(item.key) })

  await check.execute(row('uninstall-x'))
  assert.deepEqual(calls, ['uninstall-x'], 'execute 必须跑本次 run 注入的那个执行器')
  check.finish(true)
  await pending

  // 换一批：执行器跟着换，不会串到上一批
  const other = []
  const next = check.run(model(), async (item) => { other.push(item.key) })
  await check.execute(row('uninstall-y'))
  assert.deepEqual(other, ['uninstall-y'])
  assert.deepEqual(calls, ['uninstall-x'], '上一批的执行器不能再被调用')
  check.finish(true)
  await next
})

test('没有 run 就 execute 直接报错', async () => {
  const check = useInstallCheck()
  await assert.rejects(() => check.execute(row()), /no executor/)
})

/* ===== 结构：地图导入的冲突门在确认之前 ===== */

const IMPORT = readFileSync(
  new URL('../src/composables/useLevelSetImport.ts', import.meta.url),
  'utf8',
).replace(/\r\n/g, '\n')

const FINISH = (() => {
  const start = IMPORT.indexOf('async function finishStaged(')
  assert.ok(start >= 0, '找不到 finishStaged')
  const rest = IMPORT.slice(start)
  const end = rest.indexOf('\n  }\n')
  assert.ok(end > 0, '找不到 finishStaged 的函数体结尾')
  return rest.slice(0, end)
})()

test('finishStaged 里冲突处理必须早于「要不要装」的确认', () => {
  const gate = FINISH.indexOf('resolveConflicts(')
  const approval = FINISH.indexOf('askApproval(')
  assert.ok(gate >= 0, 'finishStaged 必须先处理冲突')
  assert.ok(approval >= 0, 'finishStaged 必须仍然问用户要不要装')
  assert.ok(gate < approval, '冲突处理在前、确认在后：处理完自动进入确认')
})

test('确认弹窗里的 while 卸载循环已经删掉', () => {
  assert.doesNotMatch(IMPORT, /uninstallConflicts\(/, '循环卸载已移到检查弹窗，不要再留一份')
  assert.match(IMPORT, /if \(!ready\) return null/, '检查没通过时整次导入必须中止')
  assert.match(IMPORT, /conflicts: \[\]/, '全部卸成功后要从关卡集上清掉冲突，否则详情弹窗还会提示')
})

test('冲突行按句柄去重后再交给弹窗', () => {
  assert.match(IMPORT, /if \(seen\.has\(conflict\.handle\)\) continue/, '同一个已装目录只能卸一次')
  assert.match(IMPORT, /conflicts\[row\.index\]/, '执行时用行的下标取回原始冲突项')
})

/* ===== 结构：两个宿主视图都挂上了共享弹窗 ===== */

for (const view of ['MapsOnlineView', 'LevelsView']) {
  test(`${view} 挂载冲突检查弹窗`, () => {
    const source = readFileSync(
      new URL(`../src/views/${view}.vue`, import.meta.url),
      'utf8',
    ).replace(/\r\n/g, '\n')

    assert.match(source, /import InstallCheckDialog from '\.\.\/components\/InstallCheckDialog\.vue'/)
    assert.match(source, /<InstallCheckDialog v-model:show="importCheck\.open\.value"/)
    assert.match(source, /:model="importCheck\.model\.value"/)
    assert.match(source, /:execute="importCheck\.execute"/)
    assert.match(source, /@finished="\(ready: boolean\) => \{ void importCheck\.finish\(ready\) \}"/)
    assert.match(source, /importCheck,/, '必须从 useLevelSetImport 取到弹窗状态')
  })
}
