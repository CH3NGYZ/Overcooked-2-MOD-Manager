import { backendPayloadOf, type BackendErrorPayload } from '../locale/backend.ts'
import { providerLabel } from '../providers.ts'
import type { ProviderEvidence, ProviderEvidenceRow } from './types.ts'

export interface EvidenceRow { path?: string; locate?: boolean; provider?: ProviderEvidenceRow }

export function formatEvidenceRepository(repository: NonNullable<ProviderEvidenceRow['repository']>): string {
  return `${providerLabel(repository.provider)} · ${repository.owner}/${repository.repository}`
}

export function providerEvidenceAdvice(providers: ProviderEvidence | undefined) {
  if (!providers) return undefined
  const planned = providers.rows.filter(row => row.origin === 'planned')
  if (planned.length === providers.rows.length) return 'modChanges.evidence.checkPlanned'
  if (providers.runtime_conflict && planned.length) return 'modChanges.evidence.checkRuntimePlan'
  return providers.guid === 'dev.gua.overcooked.diylevel' ? 'modChanges.evidence.checkFramework' : 'modChanges.evidence.checkCopies'
}

export function providerRows(evidence: { rows: readonly ProviderEvidenceRow[] }): EvidenceRow[] {
  return evidence.rows.map(provider => ({
    path: provider.path,
    locate: provider.installed_enabled !== undefined && /^bepinex\/plugins\/.+\.dll(?:\.disabled)?$/i.test(provider.path ?? ''),
    provider,
  }))
}

function isProviderRow(value: unknown): value is ProviderEvidenceRow {
  if (!value || typeof value !== 'object') return false
  const row = value as Partial<ProviderEvidenceRow>
  return (row.origin === 'installed' || row.origin === 'planned' || row.origin === 'selected') && typeof row.enabled === 'boolean'
    && [row.path, row.projected_path, row.node_id, row.candidate_id, row.member_key, row.version, row.label].every(field => field == null || typeof field === 'string')
    && (row.name_i18n == null || (typeof row.name_i18n === 'object' && !Array.isArray(row.name_i18n)
      && Object.values(row.name_i18n).every(name => typeof name === 'string')))
    && (row.repository == null || (['github', 'gitee', 'gitcode'].includes(row.repository.provider)
      && typeof row.repository.owner === 'string' && typeof row.repository.repository === 'string'))
    && (row.installed_enabled === undefined || typeof row.installed_enabled === 'boolean')
}

export function changeErrorEvidence(error: unknown): { providers?: ProviderEvidence; rows: EvidenceRow[] } {
  let providers: ProviderEvidence | undefined
  const rows: EvidenceRow[] = []
  let detail: BackendErrorPayload | undefined = backendPayloadOf(error)
  for (let depth = 0; detail && depth < 4; depth++) {
    const { path, paths, occupied, manifest, manifests, reason, evidence } = detail.params ?? {}
    let providerDetail = false
    if (!providers && typeof evidence === 'string') {
      try {
        const parsed = JSON.parse(evidence) as Partial<ProviderEvidence> | null
        if (typeof parsed?.guid === 'string' && Array.isArray(parsed.rows) && parsed.rows.length && parsed.rows.every(isProviderRow)) {
          providers = { guid: parsed.guid, rows: parsed.rows, ...(parsed.truncated === true ? { truncated: true } : {}),
            ...(parsed.runtime_conflict === true ? { runtime_conflict: true } : {}) }
          providerDetail = true
        }
      } catch { /* Invalid evidence cannot replace the ordinary error. */ }
    }
    const values = [path, occupied, manifest, ...[providerDetail ? undefined : paths, manifests]
      .flatMap(value => typeof value === 'string' ? value.split('\n') : [])]
    for (const value of values) {
      if (typeof value === 'string' && value && !rows.some(row => row.path === value)) rows.push({ path: value })
    }
    detail = backendPayloadOf(reason)
  }
  if (!providers) return { rows }
  const copies = providerRows(providers)
  return { providers, rows: [...copies, ...rows.filter(row => !copies.some(copy => copy.path === row.path))] }
}
