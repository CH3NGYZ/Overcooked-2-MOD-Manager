import { createI18n } from 'vue-i18n'
import { watch } from 'vue'
import { locale as appLocale } from './state.ts'
import { buildMessages } from './catalog.ts'
import { DOMAINS, LOCALE_CODES, type Domain } from './types.ts'

/** 文案登记与组装保持纯数据，Vue 实例和语言监听只在应用入口创建。 */
export const i18n = createI18n({
  // legacy: false —— 用 Composition API 形式（useI18n），与项目其余部分一致。
  // 设成 true 会让 useI18n() 抛错。
  legacy: false,
  locale: appLocale.value,
  fallbackLocale: 'zh-CN',
  // 缺翻译时不要把 tag 渲染成警告文字，退回 tag 本身更利于排查
  missingWarn: false,
  fallbackWarn: false,
  /*
   * 类型断言说明：`buildMessages` 逐层建的是普通嵌套对象，运行时就是
   * vue-i18n 要的形状；但它的静态类型是 `Record<string, unknown>`，
   * 与 vue-i18n 内部的 `LocaleMessage` 不完全一致（后者不允许任意
   * `unknown` 叶子）。这里断言成 `false` 分支期望的类型，不改变运行行为。
   */
  messages: Object.fromEntries(
    LOCALE_CODES.map((code) => [code, buildMessages(code)]),
  ) as unknown as Record<string, Record<string, string>>,
})

/*
 * 跟随应用语言。
 *
 * src/i18n.ts 的 t() 封装与 vue-i18n 共用此实例，切换语言时统一生效。
 */
watch(appLocale, (value) => {
  i18n.global.locale.value = value
}, { immediate: true })

/** 运行期自检：所有域都在组装结果里出现。 */
export function missingDomains(): Domain[] {
  const present = new Set(Object.keys(buildMessages('zh-CN')))
  return DOMAINS.filter((domain) => !present.has(domain))
}
