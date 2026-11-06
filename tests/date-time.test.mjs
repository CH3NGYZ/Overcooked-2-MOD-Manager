import assert from 'node:assert/strict'
import test from 'node:test'

/** 资产更新时间：真实源码直接执行（Node 24 类型擦除）。 */
const { formatLocalDateTime } = await import('../src/repositoryAssets.ts')

test('ISO 时间按本地时区格式化成到秒', () => {
  const iso = '2026-11-04T07:56:21Z'
  const date = new Date(iso)
  const pad = (part) => String(part).padStart(2, '0')
  const expected = `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} `
    + `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
  assert.equal(formatLocalDateTime(iso), expected)
  assert.match(formatLocalDateTime(iso), /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}$/)
})

test('空值与非法时间返回空串，调用方据此不渲染', () => {
  for (const value of ['', null, undefined, 'not-a-date']) {
    assert.equal(formatLocalDateTime(value), '')
  }
})
