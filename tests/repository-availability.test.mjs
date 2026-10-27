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

test('社区源不限地区：Gitee 社区在国际地区同样可用', () => {
  /*
   * 两个社区源默认 region 为空，因此对所有地区可用。
   * 它们与官方源的关系由后端按身份闪避（官方已收录的副本 enabled=false），
   * 不在地区维度表达。
   */
  const gitee = repo({ region: undefined, source_id: 'gitee-community' })
  const github = repo({ region: undefined, source_id: 'github-community' })
  const sources = [
    { id: 'gitee-community', region: undefined },
    { id: 'github-community', region: undefined },
  ]
  for (const region of ['chinese_mainland', 'international', undefined]) {
    assert.equal(repositoryIsAvailable(gitee, sources, region), true, `gitee ${region}`)
    assert.equal(repositoryIsAvailable(github, sources, region), true, `github ${region}`)
  }
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
