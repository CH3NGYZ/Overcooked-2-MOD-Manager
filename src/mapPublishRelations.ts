import type { RelationKind, ReleaseRelationOverride, RepositoryRelation } from './types.ts'
import { formatVersionSpec, parseVersionSpec, versionMatches } from './versionSpec.ts'
import { pickReleaseRelations } from './modRelations.ts'

export const RELATION_KINDS: readonly RelationKind[] = ['requires', 'compatible', 'conflicts']
export interface RelationRow { guid: string; range: string; prerelease?: boolean; optional?: boolean }
export type RelationTables = Record<RelationKind, RelationRow[]>
export interface PublishRelationOverride extends RelationTables { version: string }
export const recordToRows = (record: Record<string, string>): RelationRow[] =>
  Object.entries(record).map(([guid, range]) => ({ guid, range }))
export const emptyRelationTables = (): RelationTables => ({ requires: [], compatible: [], conflicts: [] })

export function relationsToRecord(rows: readonly RelationRow[]): Record<string, string> {
  return Object.fromEntries(rows.filter(row => row.guid.trim()).map(row =>
    [row.guid.trim(), formatVersionSpec(row.range) || '*']))
}

export function overrideDrafts(entries: readonly ReleaseRelationOverride[] = []): PublishRelationOverride[] {
  return entries.map(entry => {
    const tables = emptyRelationTables()
    for (const relation of entry.relations) {
      tables[relation.kind ?? 'requires'].push({ guid: relation.guid, range: relation.version ?? '',
        prerelease: relation.prerelease, optional: relation.optional })
    }
    return { version: entry.version, ...tables }
  })
}

export function buildOverrides(entries: readonly PublishRelationOverride[]): ReleaseRelationOverride[] {
  return entries.map(entry => ({ version: formatVersionSpec(entry.version),
    relations: RELATION_KINDS.flatMap(kind => entry[kind].map((row): RepositoryRelation => ({
      kind, guid: row.guid.trim(), version: formatVersionSpec(row.range),
      prerelease: row.prerelease, optional: row.optional,
    }))),
  }))
}

/** 单资产编辑只新增/复用等值覆盖，绝不把命中的宽区间改成单版本。 */
export function assetRelationOverride(
  defaults: RelationTables, entries: PublishRelationOverride[], version: string,
): number {
  const exact = entries.findIndex(entry => {
    const clauses = parseVersionSpec(entry.version)
    return clauses?.length === 1 && clauses[0]?.op === '==' && versionMatches(entry.version, version)
  })
  // 放到最前：与宽区间下限相同时，消费端按声明顺序决定优先级。
  const tables = exact >= 0 ? entries.splice(exact, 1)[0]! : overrideDrafts([{
    version: `==${version}`,
    relations: pickReleaseRelations(buildOverrides([{ version: '', ...defaults }])[0]!.relations,
      buildOverrides(entries), version),
  }])[0]!
  entries.unshift(tables)
  return 0
}

/** 返回界面字段标识；版本区间的解析复用唯一实现，不在发布页另造语法。 */
export function relationIssue(defaults: RelationTables, overrides: readonly PublishRelationOverride[]): string | null {
  const tableIssue = (tables: RelationTables, prefix: string) => {
    for (const kind of RELATION_KINDS) {
      const seen = new Set<string>()
      for (const [index, row] of tables[kind].entries()) {
        const guid = row.guid.trim().toLowerCase()
        const identity = `${guid}\u0000${String(row.prerelease)}\u0000${Boolean(row.optional)}`
        if (!guid || seen.has(identity)) return `${prefix}-${kind}-${index}-guid`
        seen.add(identity)
        if (parseVersionSpec(row.range) === null) return `${prefix}-${kind}-${index}-range`
      }
    }
    return null
  }
  const globalIssue = tableIssue(defaults, 'relation-default')
  if (globalIssue) return globalIssue
  for (const [index, entry] of overrides.entries()) {
    const prefix = `relation-override-${index}`
    const range = parseVersionSpec(entry.version)
    if (!range?.length || !RELATION_KINDS.some(kind => entry[kind].length)) return `${prefix}-version`
    const issue = tableIssue(entry, prefix)
    if (issue) return issue
  }
  return null
}
