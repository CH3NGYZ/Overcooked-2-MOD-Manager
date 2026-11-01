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
  // —— 界面域：按页面/区域归属 ——
  'common',      // 通用：确定/取消/保存/删除 等跨页面复用的词
  'nav',         // 侧栏、标题栏、页面标题
  'overlays',    // 全屏遮罩（备份/恢复/卸载整目录拷贝的等待提示）
  'overview',    // 总览
  'modsOnline',  // MOD 安装页
  'modsLocal',   // MOD 管理页
  'rename',      // 改多彩名
  'tools',       // 工具页
  'settings',    // 设置页（含框架管理抽屉）
  'framework',   // BepInEx / DIYLevel 框架管理
  'dialogs',     // 独立弹窗：确认、导入导出、回收站、日志
  'tutorial',    // 教程
  'refresh',     // 刷新批次的共享摘要（仓库 / 地图两侧同一套）

  // —— 后端域：tag 的第一段就是这里的名字 ——
  'error',       // 早期批次：`error.<域>.<原因>`
  'message',     // 早期批次：`message.<域>.<原因>`
  'lock',        // 操作锁
  'game',        // 游戏目录与运行状态
  'mods',        // MOD 扫描、开关、安装
  'modConfig',   // MOD 配置文件编辑
  'modPreview',  // MOD 预览
  'levelSet',    // DIYLevel 关卡集（地图）的扫描、开关与导入
  'assetBundle', // 关卡集 info 的 AssetBundle 解析（地图信息读不出来）
  'mapSource',   // 地图源文档校验（maps.toml 与子源）
  'backup',      // 框架备份
  'archive',     // ZIP 解压与安装
  'download',    // 下载
  'direct',      // 直链安装
  'recycle',     // 回收站
  'source',      // 仓库源与仓库
  'proxy',       // 代理
  'config',      // 配置读写
  'community',   // 社区仓库搜索
  'avatar',      // 头像
  'readme',      // 仓库说明
  'wine',        // Wine / CrossOver
  'migrate',     // 更新迁移
  'update',      // 管理器自更新
  'log',         // 日志
  'network',     // HTTP 客户端创建
] as const

export type Domain = (typeof DOMAINS)[number]

/**
 * 后端 tag 的形态。
 *
 * 后端不再返回中文，而是返回「**第一段是域**」的 tag：
 *
 * - `error.<域>.<原因>` / `message.<域>.<原因>` —— 早期迁移批次，`error` 和
 *   `message` 本身就是域，适合一眼分辨「这是错误还是成功提示」；
 * - `<域>.<原因>` —— 全量迁移后的主流形态，如 `mods.higherVersionPresent`、
 *   `game.notFound`。域直接就是 `mods` / `game`。
 *
 * 两种都合法，判定统一走 `i18n.global.te(tag)` 查表，不要用前缀白名单：
 * 漏一个前缀就会有一条文案静默漏翻。
 *
 * 路由到哪个域由第一段决定，所以 `error.mods.x` 与 `mods.x` 可以同时存在，
 * 前者进 `error` 域、后者进 `mods` 域，互不影响。
 */
