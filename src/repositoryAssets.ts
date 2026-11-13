import type { AssetRule, ReleaseAsset, ReleaseInfo } from './types'

/** Downloads only select an existing snapshot; a missing explicit/pinned version never falls back. */
export function cachedInstallRelease(
  releases: readonly ReleaseInfo[],
  tag?: string,
  pinnedVersion?: string,
): ReleaseInfo | undefined {
  if (tag !== undefined) return releases.find((release) => release.tag_name === tag)
  if (pinnedVersion !== undefined) {
    return releases.find((release) => release.tag_name === pinnedVersion || release.version === pinnedVersion)
  }
  return releases[0]
}

/**
 * 在线内容（MOD 仓库 / 地图）的资产规则匹配。
 *
 * ## 为什么单独成模块
 *
 * 这段逻辑原本只写在 `ModRepositoryDetail.vue` 的 script 里。地图下载页新加的
 * 详情抽屉同样要回答「这个版本里哪个文件会被下载」，而资产规则是**后端语义
 * 的镜像**（`exact` / `glob` / `regex`）—— 复制一份就等于允许两边慢慢分叉，
 * 出现「详情里显示了某个文件、实际下载的却是另一个」。
 *
 * 纯函数，不依赖 Vue / Naive，可被 Node 测试直接引用。
 */

/**
 * 某个文件名是否命中一条资产规则。
 *
 * 三种类型与后端 `asset_rules` 一一对应：
 *   - `exact`  忽略大小写完全相等；
 *   - `glob`   `*` / `?` 通配，其余字符按字面量处理（先转义再替换）；
 *   - `regex`  原样当作正则（不带 `g`，避免 `lastIndex` 状态串味）。
 *
 * 非法的 glob / regex 返回 `false` 而不是抛错：配置写错时该资产就是
 * 「匹配不上」，不能让它把整个详情页炸掉。
 */
export function assetMatches(name: string, rule: { type: string; value: string }): boolean {
  if (rule.type === 'exact') return name.toLocaleLowerCase() === rule.value.toLocaleLowerCase()
  if (rule.type === 'glob') {
    const escaped = rule.value.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.')
    try { return new RegExp(`^${escaped}$`, 'i').test(name) } catch { return false }
  }
  try { return new RegExp(rule.value, 'i').test(name) } catch { return false }
}

/**
 * 一个 Release 里命中资产规则的资产列表。
 *
 * 只保留带 `download_url` 的资产：没有下载地址的条目列出来也点不动，
 * 放进列表只会让人以为能装。
 */
export function matchedReleaseAssets(
  release: ReleaseInfo | null | undefined,
  rules: readonly AssetRule[],
): ReleaseAsset[] {
  if (!release) return []
  return release.assets.filter((asset) =>
    !!asset.download_url && rules.some((rule) => assetMatches(asset.name, rule)))
}

/**
 * 文件大小的展示文本（B / KB / MB / GB）。
 *
 * 缺失大小返回 `—` 而不是 `0 B`：地图和 MOD 的 Release 资产都可能没有
 * 大小元数据，显示 `0 B` 会被误读成「空文件」。
 */
export function formatFileSize(bytes: number | undefined | null): string {
  if (!bytes) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let index = 0
  while (value >= 1024 && index < units.length - 1) { value /= 1024; index += 1 }
  return `${value.toFixed(index ? 1 : 0)} ${units[index]}`
}

/**
 * 把 ISO 时间按**本地时区**格式化成 `YYYY-MM-DD HH:mm:ss`。
 *
 * Release 的 `published_at` 是 UTC（`2026-11-04T07:56:21Z`），直接截前 10 位
 * 会把「本地 11 月 4 日 15:56」显示成「11 月 4 日」甚至跨日错位。空值与
 * 解析失败返回空串，调用方据此不渲染这一项。
 */
export function formatLocalDateTime(value: string | null | undefined): string {
  if (!value) return ''
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return ''
  const pad = (part: number) => String(part).padStart(2, '0')
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}
