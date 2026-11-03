/**
 * 版本区间（pip 风格）语法的唯一前端实现。
 *
 * 关系表写 `version = ">=3.10.2,<3.12.0"`，逗号分隔的条件取 **AND**。
 * 与后端 `src-tauri/src/version_range.rs` 是同一套语法（那边是权威实现，
 * 配置文件先过后端校验），这里只负责在浏览器里判定「已装版本是否满足」。
 *
 * 排序不在本文件：`compareVersions`（`modStatus.ts`）已经处理了四段
 * beta 序号、`v` 前缀、`.beta4` 这些历史写法，两处排序规则必须只有一份。
 */
import { compareVersions } from './modStatus.ts'

export type VersionClauseOp = '==' | '!=' | '>=' | '>' | '<=' | '<'

export interface VersionClause {
  op: VersionClauseOp
  version: string
}

/**
 * 解析区间；写法不合法时返回 `null`。
 *
 * 空串与 `*` 都解析成空数组（不限版本）。空条件、缺版本号、`~=` 都算错 ——
 * 把写错的区间当成「不限」会让一条依赖永远满足，比直接报错更难查。
 */
export function parseVersionSpec(spec: string | null | undefined): VersionClause[] | null {
  const trimmed = (spec ?? '').trim()
  if (!trimmed || trimmed === '*') return []
  const clauses: VersionClause[] = []
  for (const raw of trimmed.split(',')) {
    const clause = raw.trim()
    if (!clause) return null
    const parsed = parseClause(clause)
    if (!parsed) return null
    clauses.push(parsed)
  }
  return clauses
}

function parseClause(clause: string): VersionClause | null {
  // 两字符比较符优先，否则 `>=1.0` 会被拆成 `>` + `=1.0`
  for (const token of ['>=', '<=', '!=', '==', '>', '<', '='] as const) {
    if (!clause.startsWith(token)) continue
    const version = clause.slice(token.length).trim()
    if (!isVersionToken(version)) return null
    return { op: token === '=' ? '==' : token, version }
  }
  if (!isVersionToken(clause)) return null
  return { op: '==', version: clause }
}

/** 版本号本身是否长得像个版本号（与后端 `validate_version_token` 同规则）。 */
function isVersionToken(token: string): boolean {
  if (!token) return false
  if (/\s/.test(token)) return false
  if (token.startsWith('~=')) return false
  const body = token.replace(/^[vV]/, '')
  return /^\d/.test(body) && /\d/.test(body)
}

/** 区间写法是否合法。 */
export function isValidVersionSpec(spec: string | null | undefined): boolean {
  return parseVersionSpec(spec) !== null
}

/**
 * 目标版本是否满足区间。
 *
 * 「无法判定」一律按**不满足**处理：写了条件但目标没有版本号、
 * 或版本号解析失败。无版本号绝不等于满足。
 */
export function versionMatches(
  spec: string | null | undefined,
  version: string | null | undefined,
): boolean {
  const clauses = parseVersionSpec(spec)
  if (clauses === null) return false
  if (clauses.length === 0) return true
  if (!version?.trim()) return false
  return clauses.every((clause) => {
    const compared = compareVersions(version, clause.version)
    if (compared === null) return false
    switch (clause.op) {
      case '==': return compared === 0
      case '!=': return compared !== 0
      case '>=': return compared >= 0
      case '>': return compared > 0
      case '<=': return compared <= 0
      case '<': return compared < 0
    }
  })
}

/** 规范化展示形态（`=1.2` → `==1.2`，去掉多余空白）。 */
export function formatVersionSpec(spec: string | null | undefined): string {
  const clauses = parseVersionSpec(spec)
  if (!clauses) return (spec ?? '').trim()
  return clauses.map((clause) => `${clause.op}${clause.version}`).join(',')
}

/**
 * 把旧的两端写法折成新写法（后端反序列化后前端只会收到 `version`，
 * 这个函数留给导入导出、快照兼容这类仍可能读到旧字段的地方）。
 */
export function foldVersionBounds(min: string | null | undefined, max: string | null | undefined): string {
  const lower = (min ?? '').trim()
  const upper = (max ?? '').trim()
  if (!lower && !upper) return ''
  if (lower && !upper) return `>=${lower}`
  if (!lower && upper) return `<=${upper}`
  return lower === upper ? `==${lower}` : `>=${lower},<=${upper}`
}
