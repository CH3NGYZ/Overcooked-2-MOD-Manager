import assert from 'node:assert/strict'
import test from 'node:test'

/**
 * 地图块三张多语言表的草稿逻辑（`src/mapPublishI18n.ts`）。
 *
 * 锁四件事：没填时按基本信息补默认（显示名取中英名、分组名取作者、说明留空）、
 * 块里已有的值算作者写过（默认值不再覆盖它）、基本信息改动只更新没被动过的行、
 * 以及「整张表为空不算填了、填了就必须有 zh-CN 与 en」这条必填口径。
 */
const {
  MAP_I18N_LANGUAGES,
  REQUIRED_I18N_LANGUAGES,
  blankI18nField,
  defaultI18nValues,
  extraLanguages,
  followDefaults,
  i18nDraftFrom,
  i18nTableOf,
  missingRequiredLanguages,
} = await import('../src/mapPublishI18n.ts')

const defaults = { levelSetName: 'GAODA', levelSetNameZh: '高达', author: 'gaoda' }

test('五种语言按固定顺序给出，必填只有 zh-CN 与 en', () => {
  assert.deepEqual([...MAP_I18N_LANGUAGES], ['zh-CN', 'zh-TW', 'en', 'ko', 'ja'])
  assert.deepEqual([...REQUIRED_I18N_LANGUAGES], ['zh-CN', 'en'])
})

test('显示名默认取中英名，缺一个用另一个顶上', () => {
  assert.deepEqual(defaultI18nValues('nameI18n', defaults), { 'zh-CN': '高达', en: 'GAODA' })
  // 中文名空着时中文显示名退回英文名，块里不会出现空的 zh-CN 行。
  assert.deepEqual(defaultI18nValues('nameI18n', { ...defaults, levelSetNameZh: '  ' }), {
    'zh-CN': 'GAODA',
    en: 'GAODA',
  })
  // 英文名空着时同理。
  assert.deepEqual(defaultI18nValues('nameI18n', { ...defaults, levelSetName: '' }), {
    'zh-CN': '高达',
  })
})

test('分组名默认用作者（作者不分语言），说明不填默认值', () => {
  assert.deepEqual(defaultI18nValues('groupI18n', defaults), { 'zh-CN': 'gaoda', en: 'gaoda' })
  assert.deepEqual(defaultI18nValues('groupI18n', { ...defaults, author: ' ' }), {})
  assert.deepEqual(defaultI18nValues('noteI18n', defaults), {})
})

test('块里已有的值算作者写过，默认值不覆盖；缺的行补默认', () => {
  const draft = i18nDraftFrom('nameI18n', { en: 'Custom', 'zh-CN': '自定' }, defaults)
  assert.equal(draft.values.en, 'Custom')
  assert.equal(draft.values['zh-CN'], '自定')
  // 作者没写的语言仍按默认补，界面一打开就能看见两行都有内容。
  assert.deepEqual(draft.touched, { en: true, 'zh-CN': true })

  const partial = i18nDraftFrom('nameI18n', { en: 'Custom' }, defaults)
  assert.equal(partial.values.en, 'Custom')
  assert.equal(partial.values['zh-CN'], '高达')
  assert.equal(partial.touched['zh-CN'], undefined)
})

test('基本信息改了：没动过的行跟着变，动过的行不动', () => {
  const draft = i18nDraftFrom('nameI18n', { 'zh-CN': '自定' }, defaults)
  const followed = followDefaults(draft, 'nameI18n', {
    levelSetName: 'NEW',
    levelSetNameZh: '新名',
    author: 'gaoda',
  })
  assert.equal(followed.values['zh-CN'], '自定')
  assert.equal(followed.values.en, 'NEW')
  // 不原地改入参：watch 里要拿到新对象才会触发页面更新。
  assert.equal(draft.values.en, 'GAODA')
})

test('整张表为空不算填了，填了就必须有 zh-CN 与 en', () => {
  assert.deepEqual(missingRequiredLanguages(blankI18nField()), [])
  // 只写日文：回退链不知道回哪去，两种必填都缺。
  assert.deepEqual(
    missingRequiredLanguages({ values: { ja: 'ガオダ' }, touched: {} }),
    ['zh-CN', 'en'],
  )
  assert.deepEqual(
    missingRequiredLanguages({ values: { 'zh-CN': '高达', ja: 'ガオダ' }, touched: {} }),
    ['en'],
  )
  assert.deepEqual(
    missingRequiredLanguages({ values: { 'zh-CN': '高达', en: 'GAODA' }, touched: {} }),
    [],
  )
})

test('空白值不写进块，五种之外的语言原样保留', () => {
  const draft = i18nDraftFrom('noteI18n', { 'zh-CN': '   ', fr: 'Un peu' }, defaults)
  assert.deepEqual(i18nTableOf(draft), { fr: 'Un peu' })
  assert.deepEqual(extraLanguages(draft), ['fr'])
  // 五种语言里的空值只是没填，不该算「未知语言」。
  assert.deepEqual(extraLanguages({ values: { en: '' }, touched: {} }), [])
})
