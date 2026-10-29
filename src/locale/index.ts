import { createI18n } from 'vue-i18n'
import { watch } from 'vue'
import { locale as appLocale, type Locale } from './state.ts'
import { common } from './common.ts'
import { error } from './error.ts'
import { LOCALE_CODES, type Domain, type DomainMessages, type Localized } from './types.ts'

/**
 * 分域语言表 → vue-i18n messages。
 *
 * ## 结构
 *
 * 源文件按域拆分（`common.ts` / `error.ts` / 后续的 `modsOnline.ts` …），
 * 每个域是 `{ 'tag 末段': 五语 }`。这里把它们**按语言倒排**成 vue-i18n
 * 需要的 `{ locale: { domain: { key: 文案 } } }`。
 *
 * 为什么源文件按域、而不是直接写 vue-i18n 的嵌套结构：
 * 加一条文案时要同时写五种语言，五语并排最容易发现缺项；如果按语言分文件
 * （`zh.ts` / `en.ts`），新增文案得在五个文件里各找一遍位置，漏一个就是
 * 静默回退。
 *
 * ## tag 形态
 *
 * 外部看到的 tag 是 `domain.key`，例如 `error.diyLevel.notRunning`、
 * `common.action.cancel`。点号第一段是域，其余是域内 key。
 * 后端返回的 `error.*` tag 因此可以直接喂给 `t()`。
 */
const DOMAIN_TABLES: Partial<Record<Domain, DomainMessages>> = {
  common,
  error,
}

/** 把一条五语文案按语言摊平。 */
function pick(entry: Localized, code: Locale): string {
  return entry[code]
}

/**
 * 把一个域的表展开成 vue-i18n 的**嵌套**结构。
 *
 * 关键：源表里的 key 自带点号（`diyLevel.notRunning`），而 vue-i18n 默认把
 * 点号当**嵌套路径**解析 —— 直接平铺会导致 `t('error.diyLevel.notRunning')`
 * 找不到（它去查 `error.diyLevel.notRunning` 这个字面 key，而不是
 * `error['diyLevel.notRunning']`）。所以这里按点号拆开、逐层建对象。
 *
 * 这样 tag 与 vue-i18n 的 key 形态天然一致：后端回 `error.backup.notFound`，
 * 前端直接 `t('error.backup.notFound')`，中间不需要任何映射表。
 */
function domainMessages(table: DomainMessages, code: Locale): Record<string, unknown> {
  const output: Record<string, unknown> = {}
  for (const [key, entry] of Object.entries(table)) {
    const parts = key.split('.')
    let cursor = output
    for (const part of parts.slice(0, -1)) {
      // 同一前缀已被占用成字符串时无法再展开：说明两个 tag 互相冲突，直接跳过
      if (typeof cursor[part] === 'string') continue
      cursor[part] ??= {}
      cursor = cursor[part] as Record<string, unknown>
    }
    const leaf = parts[parts.length - 1]
    cursor[leaf] = pick(entry, code)
  }
  return output
}

/** 组装某个语言下的全部域。 */
function buildMessages(code: Locale): Record<string, Record<string, unknown>> {
  const output: Record<string, Record<string, unknown>> = {}
  for (const [domain, table] of Object.entries(DOMAIN_TABLES)) {
    if (!table) continue
    output[domain] = domainMessages(table, code)
  }
  return output
}

/**
 * 已迁移到新系统的语言。
 *
 * 与旧 `src/i18n.ts` 的五语表**暂时并存**：迁移是分域推进的，未迁移的
 * 页面仍用旧 `t()`。两套共用同一个 `locale` ref，切换语言时一起变。
 * 全部域迁完后删除旧表。
 */
export const i18n = createI18n({
  // legacy: false —— 用 Composition API 形式（useI18n），与项目其余部分一致。
  // 设成 true 会让 useI18n() 抛错。
  legacy: false,
  locale: appLocale.value,
  fallbackLocale: 'zh-CN',
  // 缺翻译时不要把 tag 渲染成警告文字，退回 tag 本身更利于排查
  missingWarn: false,
  fallbackWarn: false,
  messages: Object.fromEntries(
    LOCALE_CODES.map((code) => [code, buildMessages(code)]),
  ),
})

/*
 * 跟随应用语言。
 *
 * 旧 `t()` 与新 `vue-i18n` 共用同一个 `locale` ref（`src/locale/state.ts`）：
 * 标题栏切换语言时两边必须同一帧生效，否则会出现「一半中文一半英文」
 * 的中间态。
 */
watch(appLocale, (value) => {
  i18n.global.locale.value = value
}, { immediate: true })
