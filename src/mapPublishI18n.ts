/**
 * 地图块里的三张多语言表：显示名 / 分组名 / 说明。
 *
 * 界面按语言一行一行填，五种语言固定顺序，作者能看见自己填了什么、还缺什么；
 * 以前是一个 textarea 里写 `zh-CN = 名字`，行一多就对不齐，也看不出缺哪一行。
 *
 * 纯逻辑（默认值、必填判定、未知语言保留）都在这里，页面只负责渲染与存盘，
 * Node 测试直接 import。
 */

/** 界面提供的五种语言，顺序就是行的顺序。 */
export const MAP_I18N_LANGUAGES = ['zh-CN', 'zh-TW', 'en', 'ko', 'ja'] as const
export type MapI18nLanguage = (typeof MAP_I18N_LANGUAGES)[number]

/**
 * 只强制这两种语言。
 *
 * 管理器读不到作者的语言时会自己回退（`pickI18n`：精确 → en → zh-CN → 任意），
 * 要求五语齐全只会拦着作者保存；但 zh-CN 与 en 都没有时回退链不确定，必须填。
 */
export const REQUIRED_I18N_LANGUAGES = ['zh-CN', 'en'] as const

/** 语言标签用各自的本名：作者认的是「日本語」，不是「日语」。 */
export const MAP_I18N_LANGUAGE_LABELS: Record<MapI18nLanguage, string> = {
  'zh-CN': '简体中文',
  'zh-TW': '繁體中文',
  en: 'English',
  ko: '한국어',
  ja: '日本語',
}

/** 三个多语言字段；`labelKey` 是语言表里的标题，页面与测试共用同一份顺序。 */
export const I18N_FIELD_ORDER = [
  { id: 'nameI18n', labelKey: 'mapPublish.nameI18n' },
  { id: 'groupI18n', labelKey: 'mapPublish.groupI18n' },
  { id: 'noteI18n', labelKey: 'mapPublish.noteI18n' },
] as const

export type I18nFieldId = (typeof I18N_FIELD_ORDER)[number]['id']

/**
 * 一个字段的草稿。
 *
 * `touched` 记住哪些行是作者自己写的：只有没动过的行才跟着「基本信息」的默认值
 * 走，否则改一下英文名就会把作者手写的中文显示名冲掉。
 */
export interface I18nFieldDraft {
  values: Record<string, string>
  touched: Record<string, boolean>
}

/** 默认值来自基本信息的三栏。 */
export interface I18nDefaults {
  levelSetName: string
  levelSetNameZh: string
  author: string
}

export function blankI18nField(): I18nFieldDraft {
  return { values: {}, touched: {} }
}

/**
 * 没写多语言时的默认值。
 *
 * 显示名用地图集的中英名（缺一个就用另一个顶上），分组名用作者（作者不分语言），
 * 说明留空 —— 管理器会退回块里的单语言 `note`，或显示「暂无说明」。
 */
export function defaultI18nValues(field: I18nFieldId, defaults: I18nDefaults): Record<string, string> {
  if (field === 'nameI18n') {
    const en = defaults.levelSetName.trim()
    const zh = defaults.levelSetNameZh.trim() || en
    const values: Record<string, string> = {}
    if (zh) values['zh-CN'] = zh
    if (en) values.en = en
    return values
  }
  if (field === 'groupI18n') {
    const author = defaults.author.trim()
    return author ? { 'zh-CN': author, en: author } : {}
  }
  return {}
}

/**
 * 块里的表 → 草稿。
 *
 * 块里有值的行算「作者写过」，默认值不再覆盖；五种语言之外的语言原样收进来，
 * 保存时按原值写回（作者可能手工加了 fr，界面不该悄悄删掉）。
 */
export function i18nDraftFrom(
  field: I18nFieldId,
  raw: Record<string, string> | null | undefined,
  defaults: I18nDefaults,
): I18nFieldDraft {
  const draft = blankI18nField()
  for (const [language, value] of Object.entries(raw ?? {})) {
    const text = value.trim()
    if (!text) continue
    draft.values[language] = text
    draft.touched[language] = true
  }
  for (const [language, value] of Object.entries(defaultI18nValues(field, defaults))) {
    if (!draft.touched[language]) draft.values[language] = value
  }
  return draft
}

/** 基本信息改了：只更新作者没动过的行，返回新草稿（不原地改）。 */
export function followDefaults(
  draft: I18nFieldDraft,
  field: I18nFieldId,
  defaults: I18nDefaults,
): I18nFieldDraft {
  const values = { ...draft.values }
  for (const [language, value] of Object.entries(defaultI18nValues(field, defaults))) {
    if (!draft.touched[language]) values[language] = value
  }
  return { values, touched: draft.touched }
}

/** 草稿 → 块里的表：空值不写，写了空串等于显式声明「这种语言没有名字」。 */
export function i18nTableOf(draft: I18nFieldDraft): Record<string, string> {
  const table: Record<string, string> = {}
  for (const [language, value] of Object.entries(draft.values)) {
    const text = value.trim()
    if (text) table[language] = text
  }
  return table
}

/** 五种语言之外的语言：界面只提示「会原样保留」，不提供输入行。 */
export function extraLanguages(draft: I18nFieldDraft): string[] {
  return Object.keys(i18nTableOf(draft)).filter(
    (language) => !(MAP_I18N_LANGUAGES as readonly string[]).includes(language),
  )
}

/** 这张表缺的必填语言；整张表为空时返回空数组（没填不算错，保存时按默认补）。 */
export function missingRequiredLanguages(draft: I18nFieldDraft): string[] {
  const table = i18nTableOf(draft)
  if (!Object.keys(table).length) return []
  return REQUIRED_I18N_LANGUAGES.filter((language) => !table[language])
}
