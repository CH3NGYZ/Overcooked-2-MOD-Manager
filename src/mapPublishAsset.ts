/**
 * 发布页「附件名 ← 版本号」的纯计算。
 *
 * 一个 git tag 在仓库里就是**一张地图**：消费端 `load_release_tag_maps` 直接拿
 * `release.tag_name` 当 `MapRepositoryConfig.tag`（地图身份），版本号只存在于该
 * Release 的附件名里 —— 管理器把 note 里的 `asset` 模板编译成整串锚定的正则
 * （`map_note::versioned_asset_rule`），`${version}` 那一段只匹配
 * `[0-9]+(?:\.[0-9]+)*`。所以上传的附件名必须让模板里的 `${version}` 落成
 * 「数字和点」：带 `v` 前缀或别的形状，这张图在管理器里就没有版本，更新判断也
 * 全部失效。
 *
 * 纯模块（不依赖 Vue / i18n / 组件）：`tests/map-publish-asset.test.mjs` 直接
 * import，发布页与 composable 共用同一份渲染规则。
 */

/** 管理器认的版本号形状；与 `map_note` 里版本捕获组的形状一致。 */
const MAP_VERSION_PATTERN = /^[0-9]+(?:\.[0-9]+)*$/

/** 附件模板里的版本占位符，与 `map_note::VERSION_PLACEHOLDER` 同值。 */
export const ASSET_VERSION_PLACEHOLDER = '${version}'

/** 能不能被管理器从附件名里解析出来。 */
export function isMapVersion(value: string): boolean {
  return MAP_VERSION_PATTERN.test(value.trim())
}

/**
 * 关卡集名 → 默认附件模板的前缀。
 *
 * 与后端 `save_publish_release` 在 `asset` 留空时的生成规则一致（非 ASCII
 * 字母数字一律换成 `-`，再掐掉首尾），发布页才能在上传前显示真实会落盘的名字。
 */
export function levelSetSlug(name: string): string {
  return name
    .trim()
    .split('')
    .map((item) => (/[A-Za-z0-9]/.test(item) ? item.toLowerCase() : '-'))
    .join('')
    .replace(/^-+|-+$/g, '')
}

/** 后端在模板留空时用的模板：`<slug>-${version}.zip`。 */
export function defaultTemplate(levelSetName: string): string {
  return `${levelSetSlug(levelSetName)}-${ASSET_VERSION_PLACEHOLDER}.zip`
}

/** 附件名只允许 `A-Z a-z 0-9 . _ -`；其余字符换成 `-`（GitHub 的 `?name=` 会变形）。 */
export function safeAssetName(name: string): string {
  return name
    .replace(/[^A-Za-z0-9._-]+/g, '-')
    .replace(/-{2,}/g, '-')
    .replace(/^\.+/, '')
    .slice(0, 200)
}

/**
 * 渲染这一版的附件名。
 *
 * `template` 留空时按后端规则用关卡集名生成。版本号不合法、或模板里根本没有
 * `${version}` 占位符（管理器抽不出版本）时返回空串，由调用方提示作者。
 */
export function renderAssetName(template: string, levelSetName: string, version: string): string {
  const versionText = version.trim()
  if (!isMapVersion(versionText)) return ''
  const source = template.trim() || defaultTemplate(levelSetName)
  if (!source.includes(ASSET_VERSION_PLACEHOLDER)) return ''
  return safeAssetName(source.split(ASSET_VERSION_PLACEHOLDER).join(versionText))
}

/** 附件名末尾的压缩包扩展名；只剥这些，不能把 `1.2.0` 的 `.0` 当成扩展名。 */
const ASSET_EXTENSION_PATTERN = /\.(?:zip|7z|rar)$/i

/**
 * 从附件名里反解版本号。
 *
 * 与后端 `map_publish::version_from_file_name` 一套规则（优先多段，其次单段），
 * 用来把仓库里已有的附件按版本列出来。名字里没有版本时返回空串 —— 老附件可能
 * 是 `map.zip` 这种形状，界面显示成「—」，不让它冒充版本号。
 */
export function versionFromAssetName(name: string): string {
  const stem = name.trim().replace(ASSET_EXTENSION_PATTERN, '')
  const runs = stem.match(/[0-9.]+/g) ?? []
  const trim = (run: string) => run.replace(/^\.+|\.+$/g, '')
  const valid = (run: string) => isMapVersion(trim(run))
  const multi = runs
    .slice()
    .reverse()
    .find((run) => valid(run) && trim(run).includes('.'))
  const picked = multi ?? runs.slice().reverse().find(valid)
  return picked ? trim(picked) : ''
}

/** 版本号比较（按点分段，缺段当 0）；`a` 新于 `b` 返回正数。 */
export function compareMapVersions(left: string, right: string): number {
  const toParts = (version: string) => version.split('.').map((part) => Number.parseInt(part, 10) || 0)
  const first = toParts(left)
  const second = toParts(right)
  const size = Math.max(first.length, second.length)
  for (let index = 0; index < size; index += 1) {
    const diff = (first[index] ?? 0) - (second[index] ?? 0)
    if (diff !== 0) return diff
  }
  return 0
}

/** 一组附件名里最新的版本号；一个带版本的都没有时返回空串。 */
export function latestVersionOf(names: string[]): string {
  return names
    .map(versionFromAssetName)
    .filter((version) => isMapVersion(version))
    .reduce((latest, version) => (compareMapVersions(version, latest) > 0 ? version : latest), '')
}
