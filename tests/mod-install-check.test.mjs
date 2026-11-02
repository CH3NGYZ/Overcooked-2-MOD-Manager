import assert from 'node:assert/strict'
import test from 'node:test'
import { registerHooks } from 'node:module'

/**
 * MOD 安装的「同身份冲突 → 安装前检查」纯映射。
 *
 * 锁四件事：
 *   1. 无冲突不弹检查窗 —— 否则每次安装都多一个「没问题」的窗口等着被点掉；
 *   2. 检查弹窗的目标名用包名而不是文件名 —— 用户认的是包名；
 *   3. 冲突处理完的预览只是「换了个结论」，其余字段一个都不能变：安装详情
 *      弹窗靠 `preview_id` 去确认安装、靠 `entries` / `total_dlls` 渲染，
 *      这里多做一次转换就可能把安装目标弄丢；
 *   4. 清空冲突之后，安装详情真的回到「立即安装」且不再显示冲突块 ——
 *      这是「检查弹窗已经卸过，详情弹窗不许再提一次卸载」的判据。
 *
 * ## 为什么注册解析钩子
 *
 * 第 4 条要跑真实的 `installDetail.ts`（源码里的 import 不带扩展名，交给 Vite
 * 补全，Node ESM 不补），与 `install-detail.test.mjs` 用同一套钩子。前三条
 * 只需要 `modInstallCheck.ts` 本身，它不 import 任何运行时模块。
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

const { needsInstallCheck, modCheckTarget, withConflictsResolved } =
  await import('../src/modInstallCheck.ts')
const { modPreviewDetail } = await import('../src/installDetail.ts')
const { t } = await import('../src/i18n.ts')

const conflict = (handle, name = 'Old Mod', version = '1.0.0', sameVersion = false) => ({
  handle,
  name,
  version,
  enabled: true,
  same_version: sameVersion,
})

/** 一份预览（字段与后端 `ModPreview` 一致，只给断言用得上的几项）。 */
function modPreview(overrides = {}) {
  return {
    preview_id: 'preview-1',
    kind: 'package',
    package_name: 'Better Frenzy',
    file_name: 'frenzy.zip',
    total_dlls: 2,
    mod_count: 2,
    dependency_count: 0,
    entries: [{ path: 'a.dll', role: 'mod', name: 'A Mod', version: '1.0.0' }],
    conflicts: [],
    origin: 'manual',
    release_tag: '',
    release_body: '',
    ...overrides,
  }
}

test('有冲突才弹安装前检查', () => {
  assert.equal(needsInstallCheck({ conflicts: [] }), false)
  assert.equal(needsInstallCheck({ conflicts: [conflict('a.dll')] }), true)
  assert.equal(needsInstallCheck({ conflicts: [conflict('a.dll'), conflict('b.dll')] }), true)
})

test('缺 conflicts 字段当「无冲突」，不抛', () => {
  // 防御老后端/手写对象：类型上是必填，运行时不该因为少一个字段炸掉整次安装
  assert.equal(needsInstallCheck({}), false)
  assert.equal(needsInstallCheck({ conflicts: undefined }), false)
})

test('检查弹窗的目标名优先包名，其次文件名', () => {
  assert.equal(
    modCheckTarget({ package_name: 'Better Frenzy', file_name: 'frenzy.zip' }),
    'Better Frenzy')
  // 前后空格不参与展示
  assert.equal(
    modCheckTarget({ package_name: '  Better Frenzy  ', file_name: 'frenzy.zip' }),
    'Better Frenzy')
  // 包名是空白 → 退回文件名
  assert.equal(modCheckTarget({ package_name: '   ', file_name: 'frenzy.zip' }), 'frenzy.zip')
  assert.equal(modCheckTarget({ package_name: '', file_name: '  frenzy.zip  ' }), 'frenzy.zip')
  // 两者都空：返回空串而不是 undefined，调用方拼文案时不用再判一次
  assert.equal(modCheckTarget({ package_name: '', file_name: '' }), '')
})

test('冲突处理完只清空 conflicts，安装目标与其余字段原样保留', () => {
  const preview = modPreview({ conflicts: [conflict('old.dll')] })
  const resolved = withConflictsResolved(preview)

  assert.deepEqual(resolved.conflicts, [])
  assert.equal(resolved.preview_id, 'preview-1')
  assert.equal(resolved.total_dlls, 2)
  // entries 不拷贝：安装详情只是读它，深拷贝没有意义
  assert.equal(resolved.entries, preview.entries)
  // 不能改到后端返回的原值：暂存预览还指着它，取消后要按原样清暂存
  assert.equal(preview.conflicts.length, 1)
  assert.notEqual(resolved, preview)
})

test('冲突清空后安装详情回到「立即安装」，冲突块与提示一并消失', () => {
  const preview = modPreview({ conflicts: [conflict('old.dll', '旧版厨师旋转', '2.0.0')] })
  const before = modPreviewDetail(preview)
  const after = modPreviewDetail(withConflictsResolved(preview))

  // 清空之前：按钮是「卸载并安装」，有冲突行与说明
  assert.equal(before.conflicts.length, 1)
  assert.notEqual(before.conflictHint, '')
  assert.notEqual(before.confirmLabel, t('tools.installNow'))

  // 清空之后：只剩一次纯安装
  assert.equal(after.conflicts.length, 0)
  assert.equal(after.conflictHint, '')
  assert.equal(after.confirmLabel, t('tools.installNow'))

  // 只影响冲突相关的三项：安装目标与「有没有可装内容」的判定原样
  assert.equal(after.head.name, before.head.name)
  assert.equal(after.confirmable, true)
})
