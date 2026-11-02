import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 「确认框执行中」的展示状态（`src/dialogBusy.ts`）。
 *
 * naive-ui 不会因为 `onPositiveClick` 返回 Promise 就给确定按钮加 loading，
 * 所以这层是唯一让「移入回收站 / 卸载框架 / 删包」这类慢操作有可见反馈的
 * 地方。测试锁两件事：确定按钮确实转圈、其余关闭路径确实被锁住。
 */
const { markDialogBusy } = await import('../src/dialogBusy.ts')

function fakeDialog(overrides = {}) {
  return {
    loading: false,
    closable: true,
    maskClosable: true,
    negativeButtonProps: { size: 'small', secondary: true },
    ...overrides,
  }
}

test('markDialogBusy 让确定按钮转圈，并锁住取消按钮与所有关闭路径', () => {
  const instance = fakeDialog()
  markDialogBusy(instance)
  assert.equal(instance.loading, true)
  assert.equal(instance.closable, false)
  assert.equal(instance.maskClosable, false)
  assert.equal(instance.negativeButtonProps.disabled, true)
})

test('markDialogBusy 保留调用方给取消按钮的其余 props', () => {
  const instance = fakeDialog()
  markDialogBusy(instance)
  assert.equal(instance.negativeButtonProps.size, 'small')
  assert.equal(instance.negativeButtonProps.secondary, true)
})

test('markDialogBusy 不修改调用方传入的 props 对象', () => {
  const props = { size: 'small' }
  const instance = fakeDialog({ negativeButtonProps: props })
  markDialogBusy(instance)
  assert.deepEqual(props, { size: 'small' })
  assert.notEqual(instance.negativeButtonProps, props)
})
