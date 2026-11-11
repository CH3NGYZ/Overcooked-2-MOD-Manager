/**
 * 「整理地图目录」的接线守门。
 *
 * 这条链路是**会动用户磁盘**的：把 `levels/` 下嵌套目录里的文件剪切搬进规范
 * 目录、空壳送进回收站。它的顺序不是实现细节，而是安全约束：
 *
 *   1. 先 `dry_run` 探测（只读，不要求关游戏）—— 没有候选就闭嘴，别让一个
 *      不痛不痒的按钮每次点都弹确认框；
 *   2. 有候选才弹确认，且确认框在执行期间必须锁住（`markDialogBusy`），
 *      否则用户以为没点上，再点一次就是第二遍搬文件；
 *   3. 真正搬完必须 `reloadSets()` —— 用户点这个按钮的前提就是「列表里看不到
 *      我装的地图」，不重读等于白搬。
 *
 * 这里断言的是源码文本：页面级的调用顺序没有纯函数可测（它靠 Vue 渲染与弹窗
 * 回调串起来），而这类「顺序被改回去」的退化在 `npm run build` 里完全看不出来。
 */
import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import test from 'node:test'

const PAGE = readFileSync(new URL('../src/views/LevelsView.vue', import.meta.url), 'utf8')
const API = readFileSync(new URL('../src/services/tauri.ts', import.meta.url), 'utf8')

/** 取出「整理地图目录」那一段实现（从状态声明到下一个分节注释）。 */
function organizeBlock() {
  const start = PAGE.indexOf('const organizing = ref(false)')
  assert.notEqual(start, -1, '页面里必须有整理动作的忙碌状态')
  const end = PAGE.indexOf('批量开关 / 检测更新', start)
  assert.notEqual(end, -1, '整理动作后面应该紧跟着批量开关分节')
  return PAGE.slice(start, end)
}

test('整理地图目录：先探测、再确认、最后才真搬并重读', () => {
  const block = organizeBlock()
  const probe = block.indexOf('api.flattenLevelSets(true)')
  const confirm = block.indexOf('dialog.warning({')
  const apply = block.indexOf('api.flattenLevelSets(false)')

  assert.notEqual(probe, -1, '必须先 dry_run 探测候选')
  assert.notEqual(confirm, -1, '有候选时要弹确认框')
  assert.notEqual(apply, -1, '确认后要正式整理')
  assert.ok(probe < confirm, '探测早于确认：先知道要动几处再问用户')
  assert.ok(confirm < apply, '确认早于搬移：不许点一下就动磁盘')

  // 没有候选就不打扰用户：只在探测结果为空时提示一句，不弹确认框
  assert.ok(block.includes("t('levelSet.organizeNone')"), '没有候选时给一句「不用整理」')

  // 搬移期间按钮转圈 + 弹窗锁住，避免重复触发第二次搬移
  assert.ok(block.includes('markDialogBusy(instance)'), '确认框执行期间必须锁住')
  assert.ok(block.includes('organizing.value = true'), '搬移期间按钮进入 loading')

  // 搬完必须重读列表：用户点这个按钮就是因为列表里看不到那几张图
  assert.ok(block.includes('await reloadSets()'), '整理完成后重读关卡集列表')

  // 失败要逐条说清楚，而不是一句「失败了」
  assert.ok(block.includes("t('levelSet.organizeFailed'"), '有失败项时报告数量与明细')
})

test('整理地图目录：页头按钮接的是同一个实现，且有防重入', () => {
  assert.ok(PAGE.includes('@click="organizeLevelSets"'), '页头按钮直接调整理动作')
  assert.ok(PAGE.includes(':loading="organizing"'), '按钮自己显示进度')
  assert.ok(PAGE.includes(':disabled="organizing || loading || !diyLevelAvailable"'), '列表加载中或缺少已选中的 DIYLevel 根时不允许整理')
  assert.ok(organizeBlock().includes('if (organizing.value) return'), '连点不能触发第二次')
})

test('整理地图目录：IPC 命令与 dryRun 参数名跟后端一致', () => {
  assert.ok(
    API.includes("invoke<LevelSetFlattenReport>('flatten_level_sets', { dryRun })"),
    '命令名与参数名必须和后端命令签名一致（camelCase dryRun → snake_case dry_run）',
  )
})
