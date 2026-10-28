import type { InstalledMod, Repository } from './types'

/**
 * 已安装 MOD 与仓库的双向匹配。
 * 安装清单优先记录 repository_id；旧清单或身份绑定兜底用 provider + owner/repository。
 * 两个页面（MOD 管理分组、MOD 安装状态）共用同一套判定，避免两边结论不一致。
 */
export function repositoryForMod(repositories: Repository[], mod: InstalledMod): Repository | undefined {
  if (mod.repository_id) {
    const byId = repositories.find((repo) => repo.id === mod.repository_id)
    if (byId) return byId
  }
  const binding = mod.repository_binding
  if (binding) {
    const byBinding = repositories.find((repo) =>
      repo.provider === binding.provider
      && repo.username.toLowerCase() === binding.owner.toLowerCase()
      && repo.repository.toLowerCase() === binding.repository.toLowerCase())
    if (byBinding) return byBinding
  }
  for (const id of mod.repository_ids ?? []) {
    const byId = repositories.find((repo) => repo.id === id)
    if (byId) return byId
  }
  return undefined
}

/** 同一仓库可能对应多个本地副本（旧版本共存），全部返回以便取最高版本比较。 */
export function modsForRepository(mods: InstalledMod[], repo: Repository): InstalledMod[] {
  return mods.filter((mod) => {
    if (mod.repository_id === repo.id) return true
    if (mod.repository_ids?.includes(repo.id)) return true
    const binding = mod.repository_binding
    if (!binding) return false
    return binding.provider === repo.provider
      && binding.owner.toLowerCase() === repo.username.toLowerCase()
      && binding.repository.toLowerCase() === repo.repository.toLowerCase()
  })
}

interface ParsedVersion {
  numbers: number[]
  /** 预发布标识符；空数组表示正式版。 */
  prerelease: string[]
}

/**
 * 版本号解析：三段 + 把第 4 段及以后当作「beta 序号」。
 *
 * 本项目的宽写版本号里，第 4 段是前三位版本号的 beta 序号：
 *   `1.8.7.23` ≡ `1.8.7-beta.23`（1.8.7 的第 23 个 beta）
 * 所以仍然按标准 semver 比较，只是先把多余段搬进预发布标识：
 *   - `1.8.7.23` vs `1.8.7.21` → beta.23 > beta.21 → **有更新**；
 *   - `1.8.7.23` vs `1.8.7`     → beta 低于正式版 → 已是最新；
 *   - `0.4.0.0`、`1.8.7.0`      → 序号为 0，等价于正式版；
 *   - `0.4.0.1-beta`            → `0.4.0-beta.1`（后缀在前，序号追加在后）。
 *
 * beta 写法在本项目不统一，下面四种语义完全相同，都必须识别：
 *   `1.8.5-pre.01`、`1.9.7-beta.4`、`1.8.7-beta1`、`1.8.7.beta4`
 * 所以用「贪婪吃掉所有前导数字段，剩下全部当标识符」的切法，
 * 而不是只找第一个 `-`（那样 `.beta4` 会整个解析失败）。
 *
 * 带标识时该后缀优先作为预发布标识，第 4 段追加到它后面。
 * 无法解析返回 null，调用方据此显示「版本未知」而不是臆断更新。
 */
export function parseVersion(value: string | null | undefined): ParsedVersion | null {
  if (!value) return null
  const cleaned = value.trim().replace(/^[vV]/, '')
  // 贪婪匹配前导数字段，剩下的全算后缀（兼容 -beta.4 / beta1 / .beta4）
  const match = cleaned.match(/^(\d+(?:\.\d+)*)(.*)$/)
  if (!match) return null
  const all = match[1].split('.').map(Number)
  if (all.some((part) => !Number.isFinite(part))) return null

  const numbers = all.slice(0, 3)
  while (numbers.length < 3) numbers.push(0)

  const suffix = match[2]
    .replace(/^[-+.]/, '')
    .split(/[-+.]/)
    .filter(Boolean)
  // 第 4 段及以后：全 0 视为正式版，否则作为 beta 序号
  const build = all.slice(3)
  const hasBuild = build.some((part) => part !== 0)
  const buildParts = hasBuild ? build.map(String) : []
  const prerelease = suffix.length
    ? [...suffix, ...buildParts]
    : hasBuild ? ['beta', ...buildParts] : []

  return { numbers, prerelease }
}

/**
 * 是否预发布版本。
 *
 * 两个来源取**或**：
 *   1. 后端算好的 `prerelease` 标记 —— 这是权威值，为真就一定按预发布处理，
 *      与前端如何显示版本号无关；
 *   2. 版本号本身是预发布（四段 beta 序号或 `-` 后缀）—— 仅作兜底，
 *      用于旧缓存里没有后端算过标记的 Release。
 *
 * 规则本身只在后端 `releases::is_prerelease_version` 实现一次（四段 =
 * beta 序号、`-` 后缀 = 预发布），前端这里只是同一套规则的镜像，
 * 保证「后端标记优先 + 老数据可判」两点同时成立。
 *
 * **调用方必须传 `tag_name`，不能传 `version`。** 后端的 `version` 是
 * `parse_version` 归一后的三段文本，第 4 段已经丢了，拿它反推必然得出
 * 「不是预发布」；`tag_name` 才是仓库发的原始版本字符串。
 */
export function isPrereleaseVersion(
  version: string | null | undefined,
  flag?: boolean | null,
): boolean {
  if (flag) return true
  const parsed = parseVersion(version)
  return !!parsed && parsed.prerelease.length > 0
}

/**
 * 版本号规范化显示。
 *
 * 目的只有一个：让「同一版本的不同写法」看起来一致，避免用户误以为版本不同。
 *
 * 规则只有一条：**去掉尾部全为 0 的段，其余原样保留**。
 *   - `0.4`、`0.4.0`、`0.4.0.0` → `0.4`（尾部补 0 无意义）
 *   - `1.8.7.23` → `1.8.7.23`（第 4 段有实际数字，不能省略）
 *   - `0.4.0.1-beta` → `0.4.0.1-beta`（预发布同样保留原样）
 *
 * 为什么不能截断到三段：显示要和 Release tag 逐字对上。用户看到 `1.8.7`
 * 而 tag 其实是 `1.8.7.23` 时，会以为管理器读错了版本，也无法对照 tag
 * 排查问题。比较时第 4 段按标准 semver 不参与，但显示必须忠实。
 *
 * 至少保留两段（`1.0` 不会被压成 `1`），无法解析时原样返回。
 */
export function formatVersion(value: string | null | undefined): string {
  if (!value) return ''
  const trimmed = value.trim()
  const match = trimmed.match(/^(.*?)(\d+(?:\.\d+){1,7})(?:-([0-9A-Za-z.-]+))?$/)
  if (!match) return trimmed
  const prefix = match[1]
  const numbers = match[2].split('.').map(Number)
  if (numbers.some((part) => !Number.isFinite(part))) return trimmed
  // 去掉尾部全 0 段，但至少保留两段（0.4 保持 0.4，不压成 0）
  while (numbers.length > 2 && numbers[numbers.length - 1] === 0) numbers.pop()
  const core = numbers.join('.')
  return match[3] ? `${prefix}${core}-${match[3]}` : `${prefix}${core}`
}

/** 比较版本：左新于右返回 1，相等 0，左旧于右 -1；任一方无法解析返回 null。 */
export function compareVersions(left: string | null | undefined, right: string | null | undefined): number | null {
  const a = parseVersion(left)
  const b = parseVersion(right)
  if (!a || !b) return null
  const length = Math.max(a.numbers.length, b.numbers.length)
  for (let index = 0; index < length; index += 1) {
    const delta = (a.numbers[index] ?? 0) - (b.numbers[index] ?? 0)
    if (delta !== 0) return delta > 0 ? 1 : -1
  }
  // 数字段相同：正式版高于预发布版；两个预发布版按 semver 标识符逐段比较。
  if (!a.prerelease.length && !b.prerelease.length) return 0
  if (!a.prerelease.length) return 1
  if (!b.prerelease.length) return -1
  const count = Math.max(a.prerelease.length, b.prerelease.length)
  for (let index = 0; index < count; index += 1) {
    const leftPart = a.prerelease[index]
    const rightPart = b.prerelease[index]
    if (leftPart === undefined) return -1
    if (rightPart === undefined) return 1
    const leftNumber = /^\d+$/.test(leftPart) ? Number(leftPart) : null
    const rightNumber = /^\d+$/.test(rightPart) ? Number(rightPart) : null
    if (leftNumber !== null && rightNumber !== null) {
      if (leftNumber !== rightNumber) return leftNumber > rightNumber ? 1 : -1
      continue
    }
    if (leftNumber !== null) return -1
    if (rightNumber !== null) return 1
    if (leftPart !== rightPart) return leftPart > rightPart ? 1 : -1
  }
  return 0
}
