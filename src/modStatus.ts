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
 * 版本号解析：标准 semver 语义，兼容宽写的 tag。
 *
 * 规则（与标准 semver 一致）：
 *   - 只取前三段数字参与比较，不足补 0 —— 所以 `0.4` = `0.4.0` = `0.4.0.0`；
 *   - 第 4 段及以后是构建号，按标准 semver 的语义**不参与比较**
 *     （如 `0.4.0.5` 与 `0.4.0` 相等）；
 *   - 带 `-` 后缀的是预发布，低于同号正式版：`0.4.0-beta < 0.4.0`；
 *   - 预发布标识按 semver 规则逐段比较，数字段低于字母段
 *     （`0.4.0-alpha < 0.4.0-alpha.1 < 0.4.0-beta`）。
 * 无法解析返回 null，调用方据此显示「版本未知」而不是臆断更新。
 */
export function parseVersion(value: string | null | undefined): ParsedVersion | null {
  if (!value) return null
  const cleaned = value.trim().replace(/^[vV]/, '')
  const match = cleaned.match(/(\d+(?:\.\d+){1,7})(?:-([0-9A-Za-z.-]+))?/)
  if (!match) return null
  const all = match[1].split('.').map(Number)
  if (all.some((part) => !Number.isFinite(part))) return null
  // 标准 semver 只有三段；第 4 段起是构建号，不参与比较
  const numbers = all.slice(0, 3)
  while (numbers.length < 3) numbers.push(0)
  return { numbers, prerelease: match[2] ? match[2].split('.') : [] }
}

/**
 * 是否预发布版本。
 *
 * 两个来源取**或**：
 *   1. Release 元数据里的 `prerelease` 标记（仓库作者勾选的）；
 *   2. 版本号本身带 `-` 后缀 —— `0.4.0-beta`、`0.4.0.5-beta` 这类
 *      即使作者没勾 prerelease，也应当按预发布对待。
 *
 * 任意写作里出现非数字标识符都算预发布，与 `parseVersion` 的语义一致。
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
 * 两条规则，按是否带预发布后缀区分：
 *   - **正式版**：去掉尾部多余的 `.0` 与不参与比较的第 4 段起
 *     （`0.4`、`0.4.0`、`0.4.0.0`、`0.4.0.5` 都显示 `0.4`）。
 *   - **预发布版**：**原样保留数字段**，只做必要的剥离
 *     （`v0.4.0.1-beta` → `0.4.0.1-beta`）。
 *     预发布号必须能和 Release tag 逐字对上 —— 把 `0.4.0.1-beta` 显示成
 *     `0.4-beta` 会让用户以为版本号被改了，也无法对照 tag 排查问题。
 *
 * 至少保留两段（`1.0` 不会被压成 `1`），无法解析时原样返回。
 */
export function formatVersion(value: string | null | undefined): string {
  if (!value) return ''
  const trimmed = value.trim()
  const match = trimmed.match(/^(.*?)(\d+(?:\.\d+){1,7})(?:-([0-9A-Za-z.-]+))?$/)
  if (!match) return trimmed
  const prefix = match[1]
  const all = match[2].split('.').map(Number)
  if (all.some((part) => !Number.isFinite(part))) return trimmed
  const prerelease = match[3]

  // 预发布：保留原始数字段，只去掉 v 前缀等外层包装
  if (prerelease) {
    return `${prefix}${match[2]}-${prerelease}`
  }

  // 正式版：第 4 段起是构建号，不参与比较，显示时一并忽略
  const numbers = all.slice(0, 3)
  // 去掉尾部全 0 段，但至少保留两段（0.4 保持 0.4，不压成 0）
  while (numbers.length > 2 && numbers[numbers.length - 1] === 0) numbers.pop()
  return `${prefix}${numbers.join('.')}`
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
