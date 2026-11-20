import type { InstalledMod, ModRepository, ModRepositoryBinding } from './types'

export function matchesRepositoryIdentity(repo: ModRepository, binding: ModRepositoryBinding): boolean {
  return repo.provider === binding.provider
    && repo.username.toLowerCase() === binding.owner.toLowerCase()
    && repo.repository.toLowerCase() === binding.repository.toLowerCase()
}

/**
 * 已安装 MOD 与仓库的双向匹配。
 * 安装清单优先记录 repository_id；旧清单或身份绑定兜底用 provider + owner/repository。
 * 两个页面（MOD 管理分组、MOD 安装状态）共用同一套判定，避免两边结论不一致。
 */
export function repositoryForMod(repositories: readonly ModRepository[], mod: InstalledMod): ModRepository | undefined {
  if (mod.repository_id) {
    const byId = repositories.find((repo) => repo.id === mod.repository_id)
    if (byId) return byId
  }
  const binding = mod.repository_binding
  if (binding) {
    const byBinding = repositories.find((repo) => matchesRepositoryIdentity(repo, binding))
    if (byBinding) return byBinding
  }
  for (const id of mod.repository_ids ?? []) {
    const byId = repositories.find((repo) => repo.id === id)
    if (byId) return byId
  }
  return undefined
}

/** Prefer the disk scan's reconciled version; explicit null is unknown. */
export function localVersion(mod: InstalledMod): string | undefined {
  return mod.update_version === undefined ? mod.version : mod.update_version ?? undefined
}

/** 同一仓库可能对应多个本地副本（旧版本共存），全部返回以便取最高版本比较。 */
export function modsForRepository(mods: readonly InstalledMod[], repo: ModRepository): InstalledMod[] {
  return mods.filter((mod) => {
    if (mod.repository_id === repo.id) return true
    if (mod.repository_ids?.includes(repo.id)) return true
    const binding = mod.repository_binding
    return !!binding && matchesRepositoryIdentity(repo, binding)
  })
}

interface ParsedVersion {
  numbers: number[]
  /** 预发布标识符；空数组表示正式版。 */
  prerelease: string[]
  /** tag 明确命名 alpha/beta/rc 等预发布线，区别于四段数字的旧式推断。 */
  explicitPrerelease: boolean
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
    // `beta1` 拆成 `beta` + `1`：与 `beta.1` 等价，序号才按数值比。
    // 不拆的话 `beta10` 会小于 `beta9`（单个标识符按字典序比较）。
    .flatMap((part) => {
      const digits = part.match(/^([A-Za-z]+)(\d+)$/)
      return digits ? [digits[1], String(Number(digits[2]))] : [part]
    })
  // 第 4 段及以后：全 0 视为正式版，否则作为 beta 序号
  const build = all.slice(3)
  const hasBuild = build.some((part) => part !== 0)
  const buildParts = hasBuild ? build.map(String) : []
  const prerelease = suffix.length
    ? [...suffix, ...buildParts]
    : hasBuild ? ['beta', ...buildParts] : []

  const explicitPrerelease = match[2].split('+', 1)[0].split(/[-.]/)
    .some(part => /^(?:alpha|beta|pre|preview|rc|dev|nightly)(?:\d+)?$/i.test(part))
  return { numbers, prerelease, explicitPrerelease }
}

/**
 * 是否预发布版本。
 *
 * tag 明确命名 alpha/beta/pre/preview/rc/dev/nightly 时必属预发布。其余版本优先采用平台标记
 * 或后端已解析结论（包括 false），仅未知时兼容旧快照和四段数字推断。
 * 因此数字版本取消平台预发布标记后仍能转正式，显式 beta 则不会被误分类。
 *
 * 与后端 `releases::resolve_prerelease` 保持一致，两个页面、关系判定和
 * 安装确认共用此入口，避免同一个 beta 在不同位置被当作不同发布线。
 *
 * **调用方必须传 `tag_name`，不能传 `version`。** 后端的 `version` 是
 * `parse_version` 归一后的文本可能丢失第 4 段，无法用于旧式序号推断；
 * `tag_name` 才是仓库发的原始版本字符串。
 */
export function isPrereleaseVersion(
  version: string | null | undefined,
  flag?: boolean | null,
  legacyFlag?: boolean | null,
): boolean {
  const parsed = parseVersion(version)
  if (parsed?.explicitPrerelease) return true
  if (typeof flag === 'boolean') return flag
  if (legacyFlag) return true
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
 * 排查问题。比较时第 4 段按历史 beta 序号处理，显示仍与原始版本一致。
 *
 * 至少保留两段（`1.0` 不会被压成 `1`）。本地 MOD 包标题可省略数字版本的 v/V，
 * 其他位置保留发布标签的前缀；用于比较和定位附件的原始标签不在这里改动。
 */
export function formatVersion(value: string | null | undefined, { stripVPrefix = false } = {}): string {
  if (!value) return ''
  const trimmed = stripVPrefix ? value.trim().replace(/^[vV](?=\d)/, '') : value.trim()
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

/** 数字段逐段比较，短版本缺少的尾段都按 0。 */
export function compareVersionSegments(left: readonly number[], right: readonly number[]): -1 | 0 | 1 {
  const length = Math.max(left.length, right.length)
  for (let index = 0; index < length; index += 1) {
    const a = left[index] ?? 0
    const b = right[index] ?? 0
    if (a !== b) return a > b ? 1 : -1
  }
  return 0
}

/** 比较版本：左新于右返回 1，相等 0，左旧于右 -1；任一方无法解析返回 null。 */
export function compareVersions(left: string | null | undefined, right: string | null | undefined): number | null {
  const a = parseVersion(left)
  const b = parseVersion(right)
  if (!a || !b) return null
  const order = compareVersionSegments(a.numbers, b.numbers)
  if (order) return order
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

/**
 * 版本号的两段式展示：主干 + 一个「测试标记」。
 *
 * 为什么要拆：`v1.9.9-beta.2` 这类完整写法在卡片上占很宽，而且用户真正
 * 需要的信息只有两点——「主干版本是多少」和「这是第几个测试版」。
 *
 * 规则：
 *   `1.9.9-beta.2` → base `1.9.9`,  marker `2`
 *   `1.9.9-beta`   → base `1.9.9`,  marker `beta`   （无序号，直接显示标签）
 *   `1.9.9.2`      → base `1.9.9`,  marker `2`      （四段 = beta 序号）
 *   `1.9.9`        → base `1.9.9`,  marker null     （正式版）
 *
 * `marker` 是纯数字时前端渲染成灰底数字徽标；是文字时直接拼在版本号后，
 * 提醒用户这是测试版（因为解析不出序号，无法用数字表达）。
 *
 * 传入 `prerelease` 标记：为 true 但版本号本身解析不出序号时（例如作者只是
 * 勾了 prerelease 而 tag 是 `1.9.9`），marker 用调用方给的兜底文案。
 */
export interface VersionDisplay {
  /** 主干版本（已规范化：去尾部全 0 段，但至少两段）。 */
  base: string
  /** 测试版标记：数字序号，或文字标签；正式版为 null。 */
  marker: string | null
  /** marker 是否为纯数字（决定前端用灰底数字还是文字后缀）。 */
  markerIsNumeric: boolean
}

export function versionDisplay(
  value: string | null | undefined,
  prerelease?: boolean | null,
  fallbackMarker = '',
): VersionDisplay {
  // 主干一律用 stripPrerelease 归一：去 v 前缀、去后缀、丢第 4 段序号
  const base = value ? stripPrerelease(value) : ''
  if (!isPrereleaseVersion(value, prerelease)) return { base: base || '—', marker: null, markerIsNumeric: false }
  if (!value) return { base: base || '—', marker: prerelease ? fallbackMarker : null, markerIsNumeric: false }

  const parsed = parseVersion(value)
  const raw = value.trim().replace(/^[vV]/, '')

  if (parsed && parsed.prerelease.length) {
    // 数字序号：取最后一个纯数字标识符（`beta.2` 的 2、四段的第 4 段）
    const serial = [...parsed.prerelease].reverse().find((part) => /^\d+$/.test(part))
    if (serial) {
      return { base: stripPrerelease(raw), marker: serial, markerIsNumeric: true }
    }
    // 只有文字标识（`-beta`、`-rc`）：直接拼在版本号后
    return {
      base: stripPrerelease(raw),
      marker: parsed.prerelease[parsed.prerelease.length - 1] ?? fallbackMarker,
      markerIsNumeric: false,
    }
  }

  // 版本号本身看不出预发布，但后端标记为 true：用兜底文案提示
  return { base, marker: prerelease ? fallbackMarker : null, markerIsNumeric: false }
}

/** 去掉 `v` 前缀、`-`/`+` 之后的部分，并丢掉第 4 段及以后的 beta 序号。 */
function stripPrerelease(raw: string): string {
  // 先去掉 tag 惯用的 v 前缀，否则主干会显示成 v1.9.9
  const core = raw.trim().replace(/^[vV]/, '').split(/[-+]/)[0]
  const numbers = core.split('.').filter((part) => part !== '')
  // 第 4 段是 beta 序号，不属于主干
  while (numbers.length > 3) numbers.pop()
  return formatVersion(numbers.join('.')) || core
}
