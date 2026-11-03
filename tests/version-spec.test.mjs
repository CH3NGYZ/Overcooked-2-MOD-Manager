import assert from 'node:assert/strict'
import test from 'node:test'

import {
  foldVersionBounds,
  formatVersionSpec,
  isValidVersionSpec,
  parseVersionSpec,
  versionMatches,
} from '../src/versionSpec.ts'

test('空串与 * 表示不限版本', () => {
  for (const spec of ['', '   ', '*', ' * ']) {
    assert.deepEqual(parseVersionSpec(spec), [])
    assert.equal(versionMatches(spec, ''), true)
    assert.equal(versionMatches(spec, '9.9.9'), true)
  }
})

test('上下限按 AND 组合', () => {
  const spec = '>=3.10.2,<3.12.0'
  assert.equal(versionMatches(spec, '3.10.1'), false)
  assert.equal(versionMatches(spec, '3.10.2'), true)
  assert.equal(versionMatches(spec, '3.11.9'), true)
  assert.equal(versionMatches(spec, '3.12.0'), false)
})

test('单边条件、精确匹配与排除', () => {
  assert.equal(versionMatches('>=2.0', '2.1'), true)
  assert.equal(versionMatches('>=2.0', '1.9'), false)
  assert.equal(versionMatches('<2.0', '1.9'), true)
  assert.equal(versionMatches('>2.0', '2.0'), false)
  assert.equal(versionMatches('>2.0', '2.0.1'), true)
  assert.equal(versionMatches('=1.2.3', 'v1.2.3'), true)
  assert.equal(versionMatches('==1.2.3', '1.2.4'), false)
  assert.equal(versionMatches('!=1.2.3', '1.2.4'), true)
  assert.equal(versionMatches('!=1.2.3', '1.2.3'), false)
})

test('写了条件而目标没有版本号时按不满足处理', () => {
  assert.equal(versionMatches('>=1.0', ''), false)
  assert.equal(versionMatches('>=1.0', null), false)
  assert.equal(versionMatches('==1.2.3', undefined), false)
  assert.equal(versionMatches('>=1.0', 'not-a-version'), false)
})

test('写错的区间不会被当成不限', () => {
  for (const spec of ['>=1,,<2', '>=', '~=1.2', 'abc', '>=beta']) {
    assert.equal(parseVersionSpec(spec), null, `${spec} 应当解析失败`)
    assert.equal(isValidVersionSpec(spec), false)
    assert.equal(versionMatches(spec, '1.0'), false)
  }
})

test('比较符与版本号之间的空格可以省略也可以写', () => {
  assert.equal(versionMatches('>= 1.0, < 2.0', '1.5'), true)
  assert.equal(formatVersionSpec(' =1.2 , !=1.3 '), '==1.2,!=1.3')
  assert.deepEqual(parseVersionSpec('>= 1.0'), [{ op: '>=', version: '1.0' }])
})

test('旧的两端写法可以折成 version', () => {
  assert.equal(foldVersionBounds('', ''), '')
  assert.equal(foldVersionBounds('1.0', ''), '>=1.0')
  assert.equal(foldVersionBounds('', '2.0'), '<=2.0')
  assert.equal(foldVersionBounds('1.0', '1.0'), '==1.0')
  assert.equal(foldVersionBounds('1.0', '2.0'), '>=1.0,<=2.0')
})
