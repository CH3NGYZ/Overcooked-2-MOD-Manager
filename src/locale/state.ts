/**
 * 界面语言状态（与文案表分离）。
 *
 * ## 为什么单独一个文件
 *
 * 语言状态是**纯数据**：当前选中哪个语言、可选哪些、怎么落盘、首启怎么
 * 采纳安装语言。它不该依赖任何文案表 —— 分离后有两个直接好处：
 *
 *   1. `node --test` 能直接引用它做语言切换测试，不必加载 naive-ui
 *      与几百条文案（旧的 `i18n.ts` 一被 import 就会拉进 naive-ui 的
 *      语言包，Node 环境根本跑不起来）；
 *   2. 新旧两套文案系统（旧 `t()` 与新 vue-i18n）可以共用同一个 locale，
 *      不会出现「两套各自维护一份 locale，切语言时一先一后」的中间态。
 *
 * 文案表在 `src/i18n.ts`（旧系统，迁移中）与 `src/locale/`（新系统）。
 */
import { ref, watch } from 'vue'

export type Locale = 'zh-CN' | 'zh-TW' | 'en' | 'ko' | 'ja'

const LOCALE_KEY = 'oc2-locale'
const LOCALES: Locale[] = ['zh-CN', 'zh-TW', 'en', 'ko', 'ja']

/** 是否存在浏览器存储（Node 测试环境没有 localStorage）。 */
function hasStorage(): boolean {
  return typeof localStorage !== 'undefined'
}

export const localeOptions: Array<{ value: Locale; label: string }> = [
  { value: 'zh-CN', label: '简体中文' },
  { value: 'zh-TW', label: '繁體中文' },
  { value: 'en', label: 'English' },
  { value: 'ko', label: '한국어' },
  { value: 'ja', label: '日本語' },
]

export const locale = ref<Locale>(
  hasStorage() && LOCALES.includes(localStorage.getItem(LOCALE_KEY) as Locale)
    ? (localStorage.getItem(LOCALE_KEY) as Locale)
    : 'zh-CN',
)

watch(locale, (value) => {
  // Node 测试环境没有存储与 document：只保留内存中的切换
  if (hasStorage()) localStorage.setItem(LOCALE_KEY, value)
  if (typeof document !== 'undefined') document.documentElement.lang = value
})

/** 受支持的界面语言（供安装语言的运行期校验）。 */
export const SUPPORTED_LOCALES: readonly Locale[] = LOCALES

/**
 * 是否已有用户选择过的语言。
 *
 * 「有 localStorage」= 用户在界面里选过（watch 会写）；没有则说明是首次启动，
 * 可以采纳安装程序写入的语言。
 */
export function hasStoredLocale(): boolean {
  return hasStorage() && LOCALES.includes(localStorage.getItem(LOCALE_KEY) as Locale)
}

/**
 * 应用安装程序选择的语言（仅首次启动时调用）。
 *
 * 独立成函数而不是写成模块顶层副作用：本模块被几乎所有组件间接引用，
 * 顶层发起 IPC 会让测试与纯逻辑模块也被迫依赖 Tauri。
 * 由 `main.ts` 在挂载前调用。
 *
 * 无论是否命中，首次启动都会把当前语言落盘一次：这样「用户还没选过」这个状态
 * 在启动后就不再成立，安装语言只影响第一次启动。
 */
export function applyInstallLanguage(value: string | null | undefined): boolean {
  if (hasStoredLocale()) return false
  const applied = !!value && LOCALES.includes(value as Locale)
  if (applied) {
    // 赋值触发 watch → 写入 localStorage
    locale.value = value as Locale
  } else if (hasStorage()) {
    // 没变化时 watch 不会触发，这里显式落盘默认语言
    localStorage.setItem(LOCALE_KEY, locale.value)
  }
  return applied
}
