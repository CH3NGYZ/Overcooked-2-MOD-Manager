/**
 * 发布页「附件名 ↔ 版本号」的纯计算。
 *
 * 一个 git tag 在仓库里就是**一张地图**：消费端 `load_release_tag_maps` 直接拿
 * `release.tag_name` 当 `MapRepositoryConfig.tag`（地图身份），版本号只存在于该
 * Release 的附件名里 —— 管理器把 note 里的 `asset` 模板编译成整串锚定的正则
 * （`map_note::template_rule`），`${version}` 那一段只匹配
 * `[0-9]+(?:\.[0-9]+)*`。所以上传的附件名必须让模板里的 `${version}` 落成
 * 「数字和点」，这张图在管理器里才有版本、更新判断才有效。
 *
 * v2 起作者还可以在同一块里写 `asset_versions` 排序表（内联数组，每项一对
 * `mapset_version` / `info_version`）：表里的
 * `mapset_version` 允许 `3.4.1-fix` 这种带后缀的文本（数字比较排不出 `-fix`），
 * `info_version` 记录这一版的 info 版本号。所以这里要多认一种比数字版本号更宽的
 * 「版本号文本」：`isVersionText`，带后缀的名字照样能渲染、能过输入校验。
 *
 * 纯模块（不依赖 Vue / i18n / 组件）：`tests/map-publish-asset.test.mjs` 直接
 * import，发布页与 composable 共用同一份渲染规则。
 */

/** 管理器认的版本号形状；与 `map_note` 里版本捕获组的形状一致。 */
const MAP_VERSION_PATTERN = /^[0-9]+(?:\.[0-9]+)*$/

/**
 * 「版本号文本」的形状：数字开头，后缀只允许字母、数字与 `. _ + -`。
 *
 * 数字版本号是它的子集（`isMapVersion` 为真时这里一定也为真）；多了 `-fix`
 * 这类后缀，是因为排序表按作者给的顺序认最新版，不靠数字大小比。
 */
const VERSION_TEXT_PATTERN = /^[0-9][0-9A-Za-z._+-]*$/

/** 版本号文本在正则里的写法，与 `VERSION_TEXT_PATTERN` 的字符集一致。 */
const VERSION_TEXT_SOURCE = '[0-9][0-9A-Za-z._+-]*'

/** 附件模板里的版本占位符，与 `map_note::VERSION_PLACEHOLDER` 同值。 */
export const ASSET_VERSION_PLACEHOLDER = '${version}'

/** 能不能被管理器从附件名里解析出来（数字和点）。 */
export function isMapVersion(value: string): boolean {
  return MAP_VERSION_PATTERN.test(value.trim())
}

/** 能不能当一个版本号文本用（允许 `3.4.1-fix` 这种后缀）。 */
export function isVersionText(value: string): boolean {
  return VERSION_TEXT_PATTERN.test(value.trim())
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

/** 后端与发布页共用的默认模板：`<slug>_v${version}.zip`。 */
export function defaultTemplate(levelSetName: string): string {
  return `${levelSetSlug(levelSetName)}_v${ASSET_VERSION_PLACEHOLDER}.zip`
}

/** 附件名只允许 `A-Z a-z 0-9 . _ -`；其余字符换成 `-`（GitHub 的 `?name=` 会变形）。 */
export function safeAssetName(name: string): string {
  return name
    .replace(/[^A-Za-z0-9._-]+/g, '-')
    .replace(/-{2,}/g, '-')
    .replace(/^\.+/, '')
    .slice(0, 200)
}

/** 模板渲染只校验、不改写：必须与后端 upload_asset_name 的实际名称一致。 */
export function renderAssetName(template: string, levelSetName: string, version: string): string {
  const versionText = version.trim()
  if (!isVersionText(versionText)) return ''
  const source = template.trim() || defaultTemplate(levelSetName)
  if (!source.includes(ASSET_VERSION_PLACEHOLDER)) return ''
  const name = source.split(ASSET_VERSION_PLACEHOLDER).join(versionText).trim()
  if (!name || new TextEncoder().encode(name).length > 200 || name.startsWith('.') ||
    name.endsWith('.') || /[\u0000-\u001f\u007f-\u009f/\\:*?"<>|]/.test(name)) return ''
  return name
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

/** 文件名里的最后一段；带目录的入参（POSIX 或 Windows 分隔符）都只看文件名本身。 */
function lastPathSegment(fileName: string): string {
  const parts = fileName.trim().split(/[\\/]/)
  return parts[parts.length - 1] ?? ''
}

/**
 * `xxx_v<version>.zip`：版本号段必须是版本号文本，扩展名大小写不敏感
 * （GitHub 上真的会出现 `.ZIP`）。
 */
const VERSIONED_ZIP_PATTERN = /^(.+)_v([0-9][0-9A-Za-z._+-]*)\.zip$/i

/**
 * 从 `xxx_v<version>.zip` 里拆出 tag 与版本号。
 *
 * 作者手里的包可能叫 `dmm_v3.4.1.zip`，也可能带目录。tag 去首尾空白与 `-`、`.`；
 * 去掉后为空就当作没拆出来（返回 null），由界面要求作者手填 tag 与版本号。
 */
export function parseVersionedZipName(fileName: string): { tag: string; version: string } | null {
  const match = VERSIONED_ZIP_PATTERN.exec(lastPathSegment(fileName))
  if (!match) return null
  const tag = match[1].trim().replace(/^[-.]+|[-.]+$/g, '')
  if (!tag) return null
  return { tag, version: match[2] }
}

/** 默认模板保留已存在的 tag；新建 tag 在第一步另行校验为 ASCII 字母数字。 */
export function composeAssetRule(tag: string): string {
  return `${tag.trim()}_v${ASSET_VERSION_PLACEHOLDER}.zip`
}

/** 正则元字符转义；模板里的普通字符要按字面匹配。 */
function escapeRegExp(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/**
 * 模板 → 整串锚定的正则，`${version}` 换成版本号捕获组。
 *
 * 与后端 `map_note::template_rule` 同一规则：普通字符逐字转义、`${version}` 换成
 * 版本号文本、整串锚定。模板里没有占位符时返回 null（抽不出版本，不能当规则用）。
 * 捕获组按占位符出现顺序排列，`versionFromRule` 取第一个。
 */
export function assetNamePattern(rule: string): RegExp | null {
  const template = rule.trim()
  if (!template.includes(ASSET_VERSION_PLACEHOLDER)) return null
  const pattern = template
    .split(ASSET_VERSION_PLACEHOLDER)
    .map(escapeRegExp)
    .join(`(${VERSION_TEXT_SOURCE})`)
  return new RegExp(`^${pattern}$`)
}

/** 文件名符合模板时返回抽到的版本号，否则 null。 */
export function versionFromRule(rule: string, fileName: string): string | null {
  const pattern = assetNamePattern(rule)
  if (!pattern) return null
  const match = pattern.exec(fileName.trim())
  if (!match) return null
  const version = match[1]
  return version ? version : null
}

/** 仅在已知的附件模板匹配时返回版本，保留版本后缀。 */
export function versionFromValidAsset(rule: string, fileName: string): string | null {
  return versionFromRule(rule, fileName)
}

/** 附件名里第一段数字版本号；与后端 `rule_from_asset_name` 的 `find` 同规则。 */
const ASSET_VERSION_RUN = /[0-9]+(?:\.[0-9]+)*/

/**
 * 已有地图的资产模板。
 *
 * 模板里有 `${version}` 就原样用。老地图的模板可能写死成 `xxx.zip` 或压根没写，
 * 这时从现有附件名反推：取第一个 `.zip` 附件，把它名字里第一段数字版本号换成
 * 占位符 —— 与后端 `versioned_asset_rule` 回退到 `rule_from_asset_name` 同一规则。
 * 都推不出时返回空串，由界面提示作者手填。
 */
export function assetRuleFrom(template: string, assetNames: readonly string[]): string {
  const source = template.trim()
  if (source.includes(ASSET_VERSION_PLACEHOLDER)) return source
  for (const name of assetNames) {
    const trimmed = name.trim()
    if (!trimmed.toLowerCase().endsWith('.zip')) continue
    const found = ASSET_VERSION_RUN.exec(trimmed)
    if (!found) continue
    const head = trimmed.slice(0, found.index)
    const tail = trimmed.slice(found.index + found[0].length)
    return `${head}${ASSET_VERSION_PLACEHOLDER}${tail}`
  }
  return ''
}
