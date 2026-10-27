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
 * 版本号解析：兼容 `v1.2`、`1.2.3`、`1.2.3.4`、`1.2.3-beta.1`。
 *
 * 本项目的版本号比标准 semver 宽松，允许四段甚至更多。
 * 规则：
 *   - 所有数字段都参与比较，缺的段补 0 —— 所以 `0.4` = `0.4.0`，
 *     而 `0.4.0.5` **大于** `0.4.0`（它是更具体的同一版本线）；
 *   - 只有显式带 `-` 后缀的（如 `0.4.0-beta`）才算预发布，
 *     预发布低于同号正式版。
 * 旧实现把第 4 段拼成 `-5` 当预发布，于是 `0.4.0.5 < 0.4.0`：新版被判成
 * 比旧版旧，而 DLL 是 `0.4.0.0`、tag 是 `0.4.0` 的已安装 MOD 永远显示「可更新」。
 * 无法解析返回 null，调用方据此显示「版本未知」而不是臆断更新。
 */
export function parseVersion(value: string | null | undefined): ParsedVersion | null {
  if (!value) return null
  const cleaned = value.trim().replace(/^[vV]/, '')
  const match = cleaned.match(/(\d+(?:\.\d+){1,7})(?:-([0-9A-Za-z.-]+))?/)
  if (!match) return null
  const numbers = match[1].split('.').map(Number)
  if (numbers.some((part) => !Number.isFinite(part))) return null
  // 所有数字段都保留并参与比较；只把显式 '-' 后缀当预发布
  while (numbers.length < 3) numbers.push(0)
  return { numbers, prerelease: match[2] ? match[2].split('.') : [] }
}

/**
 * 版本号规范化显示：只去掉尾部多余的 `.0`。
 *
 * 本项目版本号可以写成 `0.4`、`0.4.0`、`0.4.0.0`、`0.4.0.5`。
 * 前三种是同一版本，统一显示 `0.4`；`0.4.0.5` 是**不同的、更高的**版本，
 * 必须原样显示，否则用户看不出它比 `0.4.0` 新。
 *
 * 保留预发布后缀（`-beta.1`），至少保留两段（`1.0` 不会被压成 `1`），
 * 无法解析时原样返回。
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
