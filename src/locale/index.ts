import { createI18n } from 'vue-i18n'
import { watch } from 'vue'
import { locale as appLocale, type Locale } from './state.ts'
import { common } from './common.ts'
import { config } from './config.ts'
import { error } from './error.ts'
import { framework } from './framework.ts'
import { frameworkBackend } from './frameworkBackend.ts'
import { game } from './game.ts'
import { archive, direct, download, recycle } from './installPipeline.ts'
import { message } from './message.ts'
import {
  avatar,
  errorExtra,
  lock,
  log,
  networkClient,
  readme,
  renameBackend,
  toolsBackend,
  tutorialBackend,
  wine,
} from './misc.ts'
import { mods } from './mods.ts'
import { backup, modConfig, modPreview } from './modsFiles.ts'
import { levelSet } from './levelSet.ts'
import { community, proxy } from './network.ts'
import { source } from './source.ts'
import { migrate, update } from './update.ts'
import { nav } from './nav.ts'
import { overview } from './overview.ts'
import { frameworkUi } from './frameworkUi.ts'
import { overlays } from './overlays.ts'
import { recycleUi } from './recycleUi.ts'
import { modsOnline } from './modsOnline.ts'
import { modsLocal } from './modsLocal.ts'
import { settings } from './settings.ts'
import { toolsUi } from './toolsUi.ts'
import { dialogs } from './dialogs.ts'
import { renameUi } from './renameUi.ts'
import { uiCommon } from './uiCommon.ts'
import { DOMAINS, LOCALE_CODES, type Domain, type DomainMessages, type Localized } from './types.ts'

/**
 * 分域语言表 → vue-i18n messages。
 *
 * ## 结构
 *
 * 源文件按域拆分，每个域是 `{ 'tag 末段': 五语 }`。这里把它们**按语言倒排**
 * 成 vue-i18n 需要的 `{ locale: { domain: { key: 文案 } } }`。
 *
 * 为什么源文件按域、而不是直接写 vue-i18n 的嵌套结构：
 * 加一条文案时要同时写五种语言，五语并排最容易发现缺项；如果按语言分文件
 * （`zh.ts` / `en.ts`），新增文案得在五个文件里各找一遍位置，漏一个就是
 * 静默回退。
 *
 * ## tag 形态
 *
 * 后端返回的 tag 第一段就是域：
 *
 * - `mods.higherVersionPresent` → 域 `mods`，key `higherVersionPresent`
 * - `error.mods.deleted` → 域 `error`，key `mods.deleted`
 * - `message.backup.deleted` → 域 `message`，key `backup.deleted`
 *
 * 所以组装时**按 key 自身的第一段**路由，而不是按源文件名 —— 一个源文件
 * 可以同时容纳几张表（如 `installPipeline.ts` 导出了 archive / download /
 * direct / recycle 四张）。
 *
 * ## 为什么必须建嵌套对象
 *
 * vue-i18n 默认把点号当**嵌套路径**解析：平铺的 `'mods.higherVersionPresent'`
 * 作为字面 key 是取不到的（它会去找 `mods` 对象下的 `higherVersionPresent`）。
 * 所以这里按点号拆开、逐层建对象，让 tag 与 vue-i18n 的 key 形态天然一致。
 */
/**
 * 一张表 + 它负责的域。
 *
 * 表里一律写**域内短 key**（`higherVersionPresent`）或者**完整 tag**
 * （`mods.dllNotFound`），由 `flat` 区分：
 *
 * - `{ domain: 'mods', messages: mods }` —— 短 key，域在这里声明；
 * - `{ flat: true, messages: error }` —— key 自带完整域名。
 *
 * 为什么需要 `flat`：早期批次把「错误 / 成功提示」当成域，`error.ts` 与
 * `message.ts` 的 key 写成了 `mods.dllNotFound` 这种「域.key」形态。后端
 * 现在统一发裸 tag，这两张表正好可以直接用 key 自身查到，不需要再加前缀。
 * 靠「key 有没有点号」猜域是不行的 —— 短 key 也能带点。
 */
interface DomainTable {
  domain?: Domain
  flat?: boolean
  messages: DomainMessages
}

const TABLES: DomainTable[] = [
  { domain: 'common', messages: common },
  { domain: 'config', messages: config },
  { flat: true, messages: error },
  { flat: true, messages: errorExtra },
  { domain: 'framework', messages: framework },
  { domain: 'framework', messages: frameworkBackend },
  { domain: 'game', messages: game },
  { domain: 'archive', messages: archive },
  { domain: 'download', messages: download },
  { domain: 'direct', messages: direct },
  { domain: 'recycle', messages: recycle },
  { domain: 'message', messages: message },
  { domain: 'avatar', messages: avatar },
  { domain: 'readme', messages: readme },
  { domain: 'wine', messages: wine },
  { domain: 'tutorial', messages: tutorialBackend },
  { domain: 'tools', messages: toolsBackend },
  { domain: 'lock', messages: lock },
  { domain: 'log', messages: log },
  { domain: 'network', messages: networkClient },
  { domain: 'rename', messages: renameBackend },
  { domain: 'mods', messages: mods },
  { domain: 'backup', messages: backup },
  { domain: 'modConfig', messages: modConfig },
  { domain: 'modPreview', messages: modPreview },
  { domain: 'levelSet', messages: levelSet },
  { domain: 'community', messages: community },
  { domain: 'proxy', messages: proxy },
  { domain: 'source', messages: source },
  { domain: 'migrate', messages: migrate },
  { domain: 'update', messages: update },

  // —— 界面文案（迁移自旧 `src/i18n.ts` 的中文 key 表）——
  { domain: 'nav', messages: nav },
  { domain: 'overview', messages: overview },
  { domain: 'framework', messages: frameworkUi },
  { domain: 'overlays', messages: overlays },
  { domain: 'recycle', messages: recycleUi },
  { domain: 'modsOnline', messages: modsOnline },
  { domain: 'modsLocal', messages: modsLocal },
  { domain: 'settings', messages: settings },
  { domain: 'tools', messages: toolsUi },
  { domain: 'dialogs', messages: dialogs },
  { domain: 'rename', messages: renameUi },
  { domain: 'common', messages: uiCommon },
]

/** 把一条五语文案按语言摊平。 */
function pick(entry: Localized, code: Locale): string {
  return entry[code]
}

/**
 * 把一条「域 + 域内 key」写进某个语言的嵌套对象。
 *
 * `domain` 是 tag 的第一段（`mods` / `error` / `modConfig`），`key` 是
 * 其后的部分，可以再带点号（`error` 域里写 `mods.deleted`）。
 */
function assign(output: Record<string, unknown>, domain: string, key: string, text: string): void {
  const parts = [...(domain ? domain.split('.') : []), ...key.split('.')]
  let cursor = output
  for (const part of parts.slice(0, -1)) {
    const next = cursor[part]
    if (typeof next === 'string') return
    if (typeof next !== 'object' || next === null) cursor[part] = {}
    cursor = cursor[part] as Record<string, unknown>
  }
  cursor[parts[parts.length - 1]] = text
}

/** 组装某个语言下的全部域。 */
function buildMessages(code: Locale): Record<string, unknown> {
  const output: Record<string, unknown> = {}
  for (const { domain, flat, messages } of TABLES) {
    for (const [key, entry] of Object.entries(messages)) {
      if (flat) assign(output, '', key, pick(entry, code))
      else if (domain) assign(output, domain, key, pick(entry, code))
    }
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
 * 旧 `t()` 与新 `vue-i18n` 共用同一个 `locale` ref（`src/locale/state.ts`）：
 * 标题栏切换语言时两边必须同一帧生效，否则会出现「一半中文一半英文」
 * 的中间态。
 */
watch(appLocale, (value) => {
  i18n.global.locale.value = value
}, { immediate: true })

/** 运行期自检：所有域都在组装结果里出现。 */
export function missingDomains(): Domain[] {
  const present = new Set(Object.keys(buildMessages('zh-CN')))
  return DOMAINS.filter((domain) => !present.has(domain))
}
