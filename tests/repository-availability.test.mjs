import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 仓库「是否存在」：地区与 enabled 共同决定，且必须与后端一致。
 * 真实源码直接执行（不 import i18n / naive-ui）。
 */
const {
  repositoryIsAvailable,
  repositoryRegionMatches,
  sourceMatchesRegion,
} = await import('../src/repositoryAvailability.ts')

const sources = [
  { id: 'official', region: 'chinese_mainland' },
  { id: 'intl-source', region: 'international' },
  { id: 'any-source', region: undefined },
  { id: 'gitee-community', region: 'chinese_mainland' },
]

/** 默认所有属性都「可用」，逐项覆盖。 */
const repo = (extra = {}) => ({
  enabled: true,
  region: undefined,
  source_id: 'any-source',
  ...extra,
})

test('enabled=false 在两种地区都不可用', () => {
  for (const region of ['chinese_mainland', 'international']) {
    assert.equal(repositoryIsAvailable(repo({ enabled: false }), sources, region), false, region)
  }
})

test('international 仓库在中国大陆不可用', () => {
  const intl = repo({ region: 'international' })
  assert.equal(repositoryIsAvailable(intl, sources, 'chinese_mainland'), false)
  assert.equal(repositoryIsAvailable(intl, sources, 'international'), true)
})

test('chinese_mainland 仓库在国际地区不可用', () => {
  const cn = repo({ region: 'chinese_mainland' })
  assert.equal(repositoryIsAvailable(cn, sources, 'chinese_mainland'), true)
  assert.equal(repositoryIsAvailable(cn, sources, 'international'), false)
})

test('地区为空的仓库在两种地区都可用', () => {
  const any = repo()
  assert.equal(repositoryIsAvailable(any, sources, 'chinese_mainland'), true)
  assert.equal(repositoryIsAvailable(any, sources, 'international'), true)
})

test('所属来源地区不适用时仓库也不可用', () => {
  // 仓库自身没写地区，但它挂在一个 international 子源下
  const child = repo({ source_id: 'intl-source' })
  assert.equal(repositoryIsAvailable(child, sources, 'chinese_mainland'), false)
  assert.equal(repositoryIsAvailable(child, sources, 'international'), true)
})

test('来源已从配置删除时仓库不可用（与后端一致）', () => {
  assert.equal(repositoryIsAvailable(repo({ source_id: 'gone' }), sources, 'chinese_mainland'), false)
})

test('Gitee 社区源只在中国大陆可用，即使它没写地区', () => {
  const gitee = repo({ source_id: 'gitee-community' })
  assert.equal(repositoryIsAvailable(gitee, [{ id: 'gitee-community', region: undefined }], 'chinese_mainland'), true)
  assert.equal(repositoryIsAvailable(gitee, [{ id: 'gitee-community', region: undefined }], 'international'), false)
})

test('没有 source_id 的仓库只看 enabled 与自身地区', () => {
  const standalone = repo({ source_id: undefined })
  assert.equal(repositoryIsAvailable(standalone, sources, 'chinese_mainland'), true)
  assert.equal(repositoryIsAvailable(standalone, sources, 'international'), true)
  assert.equal(repositoryIsAvailable({ ...standalone, enabled: false }, sources, 'international'), false)
})

test('旧地区别名按归一化后的值判断', () => {
  assert.equal(repositoryRegionMatches('other', 'international'), true)
  assert.equal(repositoryRegionMatches('other', 'chinese_mainland'), false)
  assert.equal(repositoryRegionMatches('mainland-china', 'chinese_mainland'), true)
  assert.equal(sourceMatchesRegion({ region: 'other' }, 'chinese_mainland'), false)
})

test('当前地区为空按「非中国大陆」处理，与后端 current_user_region 一致', () => {
  const intl = repo({ region: 'international' })
  assert.equal(repositoryIsAvailable(intl, sources, undefined), true)
  assert.equal(repositoryIsAvailable(repo({ region: 'chinese_mainland' }), sources, undefined), false)
})
