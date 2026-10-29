/**
 * 语言表的域划分与类型约定。
 *
 * ## 为什么用 tag 而不是中文当 key
 *
 * 旧实现用**中文原文**当 key（`t('请打开游戏并确认 街机MOD 正在运行')`）。
 * 三个问题：
 *   1. 后端也返回中文串，两边的字面量必须逐字节一致才能对上，
 *      改一个标点就静默退回中文（真实发生过：代码写「解析」、
 *      语言表登记「安装」，切语言不变）；
 *   2. 中文 key 无法表达参数位置，只能靠 `{name}` 手工插值；
 *   3. 语言表按行号堆在一个文件里，谁负责哪一块看不出来。
 *
 * 现在统一改成**语义化 tag**：`domain.section.key`，例如
 * `rename.push.failed`、`error.diyLevel.notRunning`。
 * 后端只回 tag（`error.backup.restoreFailed`），前端一次性翻译。
 *
 * ## 域划分
 *
 * 每个域一个文件，按使用它的界面区域归属，而不是按控件类型。
 * 新增文案时先找它「在哪个页面出现」，再加到对应域文件。
 */
export type Locale = 'zh-CN' | 'zh-TW' | 'en' | 'ko' | 'ja'

/** 全部语言代码（顺序即 `Localized` 字段顺序之外的另一份清单，供组装用）。 */
export const LOCALE_CODES: readonly Locale[] = ['zh-CN', 'zh-TW', 'en', 'ko', 'ja'] as const

/** 单条文案在五种语言下的取值。顺序固定，与 `Locale` 一一对应。 */
export interface Localized {
  'zh-CN': string
  'zh-TW': string
  en: string
  ko: string
  ja: string
}

/** 一个域的全部文案：tag 的末段 → 五语。 */
export type DomainMessages = Record<string, Localized>

/**
 * 域清单。
 *
 * 用 `as const` 保留字面量类型，方便后续做 tag 的编译期校验
 * （`domain.key` 的组合是否真的存在）。
 */
export const DOMAINS = [
  'common',      // 通用：确定/取消/保存/删除 等跨页面复用的词
  'nav',         // 侧栏、标题栏、页面标题
  'overview',    // 总览
  'modsOnline',  // MOD 安装页
  'modsLocal',   // MOD 管理页
  'rename',      // 改多彩名
  'tools',       // 工具页
  'settings',    // 设置页（含框架管理抽屉）
  'dialogs',     // 独立弹窗：确认、导入导出、回收站、日志
  'tutorial',    // 教程
  'error',       // **后端返回的错误/状态 tag**，见下
] as const

export type Domain = (typeof DOMAINS)[number]

/**
 * 后端错误 tag 的约定。
 *
 * 后端不再返回中文，而是返回 `error.<域>.<原因>` 形式的 tag；
 * 前端查 `error` 域取文案。带参数时后端把参数一并回传，形如
 * `error.backup.restoreFailed` + `{ path: 'D:\\...' }`。
 *
 * 为什么不用 HTTP 状态码那套数字码：tag 自解释，日志里一眼能看出
 * 是哪个环节失败，不需要对照表。
 */
export const ERROR_TAG_PREFIX = 'error.'
