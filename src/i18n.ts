import { computed } from 'vue'
import { dateEnUS, dateJaJP, dateKoKR, dateZhCN, dateZhTW, enUS, jaJP, koKR, zhCN, zhTW } from 'naive-ui'

/**
 * 界面文案的取词入口。
 *
 * ## 历史与现状
 *
 * 这里曾经存着整套以**中文原文为 key** 的五语表（491 条，`t('已安装 MOD')`）。
 * 那种做法的问题：后端也返回中文时两边必须逐字节一致，改一个标点就静默
 * 失效；而且 491 条堆在一个文件里，看不出谁属于哪个页面。
 *
 * 现在文案全部在 `src/locale/` 按域分文件，`t()` 只是 **vue-i18n 的一层
 * 薄封装**，保留是为了让 ~500 个既有调用点不用同时改动。新代码可以直接
 * 用 `useI18n()`，两者查的是同一份消息表。
 *
 * ## 与后端 tag 的关系
 *
 * `t()` 既可以取界面文案（`t('nav.overview')`），也可以取后端返回的
 * tag（`t('mods.higherVersionPresent')`）—— 它们在同一个命名空间里。
 * 但**后端返回值必须走 `translateBackendMessage()`**，因为那边还要处理
 * 内联参数（`tag?a=1&b=2`）和「不是 tag 就原样返回」的兜底。
 *
 * ## 语言状态
 *
 * `locale` 是独立模块（`src/locale/state.ts`），让纯逻辑能被 Node 测试
 * 直接引用；vue-i18n 监听同一个 ref，切换语言时两边同一帧生效。
 */
import {
  applyInstallLanguage,
  hasStoredLocale,
  locale,
  localeOptions,
  SUPPORTED_LOCALES,
  type Locale,
} from './locale/state'
import { i18n } from './locale/index.ts'

export {
  applyInstallLanguage,
  hasStoredLocale,
  locale,
  localeOptions,
  SUPPORTED_LOCALES,
  type Locale,
}

/**
 * 取翻译；支持 `{placeholder}` 替换。
 *
 * 未知 key 原样返回 —— 宁可让开发者看到 `mods.someTag` 这个裸 tag，
 * 也不要显示空白（空白会让人以为是布局问题）。
 */
export function t(key: string, params?: Record<string, string | number>): string {
  return params
    ? i18n.global.t(key, params as Record<string, unknown>)
    : i18n.global.t(key)
}

/** Naive UI 组件文案。 */
export const naiveLocale = computed(() => ({
  'zh-CN': zhCN, 'zh-TW': zhTW, en: enUS, ko: koKR, ja: jaJP,
}[locale.value]))
export const naiveDateLocale = computed(() => ({
  'zh-CN': dateZhCN, 'zh-TW': dateZhTW, en: dateEnUS, ko: dateKoKR, ja: dateJaJP,
}[locale.value]))

/** v2 语言选择器标签（随当前语言显示本族名）。 */
export const currentLocaleLabel = computed(() => ({
  'zh-CN': '简体中文', 'zh-TW': '繁體中文', en: 'English', ko: '한국어', ja: '日本語',
}[locale.value]))

export function setLocale(value: Locale) {
  locale.value = value
}

/**
 * 从多语言映射里按当前界面语言取文案，缺失时回退。
 *
 * 回退顺序：完整标记 → `zh-CN` → 任意第一个非空值 → `fallback`。
 * 取「任意第一个」是为了兼容源文档只写了一两种语言的情况——
 * 显示一个外语名称总比显示空白好。
 *
 * 这里的映射来自**仓库源文档**（`name_i18n` 等），不是界面文案，
 * 所以不走 vue-i18n。
 */
export function pickI18n(
  map: Record<string, string> | undefined | null,
  fallback: string,
): string {
  if (!map) return fallback
  const exact = map[locale.value]
  if (exact && exact.trim()) return exact
  const chinese = map['zh-CN']
  if (chinese && chinese.trim()) return chinese
  for (const value of Object.values(map)) {
    if (value && value.trim()) return value
  }
  return fallback
}
