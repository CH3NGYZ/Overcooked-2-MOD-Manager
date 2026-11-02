import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * MOD 安装的「同身份冲突 → 安装前检查」纯映射。
 *
 * 锁三件事：
 *   1. 无冲突不弹检查窗 —— 否则每次安装都多一个「没问题」的窗口等着被点掉；
 *   2. 检查弹窗的目标名用包名而不是文件名 —— 用户认的是包名；
 *   3. 冲突处理完的预览只是「换了个结论」，其余字段一个都不能变：安装详情
 *      弹窗靠 `preview_id` 去确认安装、靠 `entries` / `total_dlls` 渲染，
 *      这里多做一次转换就可能把安装目标弄丢。
 */
const { needsInstallCheck, modCheckTarget, withConflictsResolved } =
  await import('../src/modInstallCheck.ts')

const conflict = (handle, name = 'Old Mod', version = '1.0.0', sameVersion = false) => ({
  handle,
  name,
  version,
  enabled: true,
  same_version: sameVersion,
})

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
  const preview = {
    preview_id: 'p-1',
    package_name: 'Better Frenzy',
    file_name: 'frenzy.zip',
    total_dlls: 2,
    entries: [{ file_name: 'a.dll' }],
    conflicts: [conflict('old.dll')],
  }
  const resolved = withConflictsResolved(preview)

  assert.deepEqual(resolved.conflicts, [])
  assert.equal(resolved.preview_id, 'p-1')
  assert.equal(resolved.total_dlls, 2)
  // entries 不拷贝：安装详情只是读它，深拷贝没有意义
  assert.equal(resolved.entries, preview.entries)
  // 不能改到后端返回的原值：暂存预览还指着它，取消后要按原样清暂存
  assert.equal(preview.conflicts.length, 1)
  assert.notEqual(resolved, preview)
})
