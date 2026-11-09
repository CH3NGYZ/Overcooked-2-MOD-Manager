/**
 * MOD 管理 / 地图管理的搜索文本。
 *
 * 只收卡片上给用户看、能用来认出这一项的文字：名称的各种语言写法、作者、
 * 分类、包名、备注、版本号、绑定来源，以及随包插件的名字与版本。内部数据
 * 一律不进 haystack —— 路径与目录名、GUID/MD5、内部 id、状态枚举、布尔开关、
 * URL、时间戳这些要么让随手一个词命中一大片无关项，要么把用户引向实现细节。
 * 状态本身有页头标签可以定位，不需要混进文本搜索。
 *
 * 纯模块（不依赖 Vue / i18n）：`tests/local-item-search.test.mjs` 直接跑源码。
 */
import { formatVersion } from './modStatus.ts'
import type { InstalledMod, LevelSetInfo } from './types'

type SearchValue = string | null | undefined | Record<string, string>

/** 可选字段与多语言表拼成一根 haystack；空值不占位，避免把空串当命中。 */
function join(values: SearchValue[]): string {
  const parts: string[] = []
  for (const value of values) {
    if (!value) continue
    if (typeof value === 'string') {
      const text = value.trim()
      if (text) parts.push(text)
    } else {
      for (const text of Object.values(value)) {
        if (text?.trim()) parts.push(text.trim())
      }
    }
  }
  return parts.join(' ').toLowerCase()
}

/**
 * 版本号两种写法都收：卡片用 `formatVersion` 把 `1.8.7.0` 显示成 `1.8.7`，
 * 用户照屏幕抄或照 Release tag 抄，都得命中同一项。
 */
function versionTexts(value?: string | null): string[] {
  if (!value?.trim()) return []
  const raw = value.trim()
  const shown = formatVersion(raw)
  return shown && shown !== raw ? [raw, shown] : [raw]
}

export function modSearchText(mod: InstalledMod): string {
  return join([
    mod.title,
    mod.name,
    mod.product_name,
    mod.plugin_name,
    mod.original_filename,
    mod.description,
    mod.company,
    mod.copyright,
    mod.group,
    mod.group_i18n,
    mod.package_name,
    mod.package_name_i18n,
    ...versionTexts(mod.version),
    mod.note,
    mod.repository_binding?.owner,
    mod.repository_binding?.repository,
    mod.repository_binding?.tag,
  ])
}

export function levelSetSearchText(set: LevelSetInfo): string {
  return join([
    set.name,
    set.name_zh,
    ...(set.levels ?? []).flatMap(level => [level.name, level.name_zh]),
    set.author,
    set.map_group,
    set.map_group_i18n,
    ...versionTexts(set.version),
    ...versionTexts(set.installed_mapset_version),
    ...(set.plugins ?? []).flatMap((plugin) => [
      plugin.name,
      plugin.title,
      plugin.product_name,
      plugin.company,
      plugin.file_name,
      ...versionTexts(plugin.version),
    ]),
  ])
}

/**
 * 空词不过滤。页面才知道的文字（卡片显示标题、绑定地图名与 tag）由 `extras`
 * 拼进来，纯模块与展示逻辑各管各的。
 */
export function matchesLocalSearch(
  query: string,
  text: string,
  ...extras: SearchValue[]
): boolean {
  const needle = query.trim().toLowerCase()
  if (!needle) return true
  return `${text} ${join(extras)}`.includes(needle)
}
