<script setup lang="ts">
import { computed, type DeepReadonly } from 'vue'
import { ArrowUndoOutline, CheckmarkOutline, CloseOutline, EllipsisHorizontal } from '@vicons/ionicons5'
import { NSpin, NTag } from 'naive-ui'
import { locale, t } from '../i18n'
import { DEPENDENCY_ISSUE_TEXT } from '../modRelations'
import { formatVersionSpec } from '../versionSpec'
import { changeReasonText } from './messages'
import { changePlanDetails, type ChangePlanNames } from './presentation'
import type { ChangeExecutionStage, ChangePlan, ChangePreparationProgress, DiagnosticStep } from './types'

const { plan, progress, execution, names, preparationComplete } = defineProps<{
  plan: DeepReadonly<ChangePlan>
  progress: DeepReadonly<ChangePreparationProgress> | null
  execution: ReadonlyMap<string, ChangeExecutionStage>
  names: ChangePlanNames
  preparationComplete: boolean
}>()

const details = computed(() => changePlanDetails(plan, progress, names))
const listFormat = computed(() => new Intl.ListFormat(locale.value, { style: 'short', type: 'conjunction' }))
type Row = ReturnType<typeof changePlanDetails>['rows'][number]
type Reason = NonNullable<Row['reasons']>[number]
type Edge = NonNullable<NonNullable<Row['diagnostic']>['edge']>
const stageLabels = {
  queued: 'modChanges.item.queued', prepared: 'modChanges.item.prepared',
  applying: 'modChanges.apply.applying', processed: 'modChanges.apply.processed',
  applied: 'modChanges.apply.applied', failed: 'modChanges.apply.failed',
  rolled_back: 'modChanges.apply.rolledBack', not_applied: 'modChanges.apply.notApplied',
  downloading: 'modChanges.item.downloading', verifying: 'modChanges.item.verifying', preparationFailed: 'modChanges.item.failed',
} as const

const rows = computed(() => details.value.rows.map(row => {
  const terminal = row.diagnostic?.terminal
  if (terminal) {
    const reason = diagnosticReason(terminal)
    return { ...row, terminalReason: reason, status: { stage: 'blocked', label: t('modChanges.diagnostic.blocked', { reason }), icon: 'error' } }
  }
  if (plan.status !== 'ready' || (!row.statusKey && !row.preparationStage)) return { ...row, terminalReason: undefined, status: undefined }
  const applying = row.statusKey ? execution.get(row.statusKey) : undefined
  const prepared = row.preparationStage
  const stage = applying ?? (preparationComplete && row.waitsForPackage ? 'prepared'
    : prepared === 'failed' ? 'preparationFailed' : prepared === 'ready' ? 'prepared' : prepared ?? 'queued')
  const icon = stage === 'failed' || stage === 'preparationFailed' ? 'error'
    : stage === 'applying' || stage === 'downloading' || stage === 'verifying' ? 'spinner'
    : stage === 'applied' || stage === 'processed' || stage === 'prepared' ? 'check'
    : stage === 'rolled_back' ? 'undo' : 'dots'
  return { ...row, terminalReason: undefined, status: { stage, label: t(stageLabels[stage]), icon } }
}))

function diagnosticReason({ state, reason, relation }: DeepReadonly<DiagnosticStep>) {
  if (reason === 'candidate_relations_unknown') return changeReasonText(reason)
  if (reason === 'duplicate_enabled_providers') return t('modChanges.diagnostic.duplicateProviders')
  if (reason === 'modChanges.multipleInstalledCopies') return t('modChanges.multipleInstalledCopies')
  switch (state) {
    case 'missing':
    case 'disabled':
    case 'version_mismatch': return t(DEPENDENCY_ISSUE_TEXT[state])
    case 'conflict': return t(relation?.kind === 'compatible' ? 'modsOnline.incompatible' : 'modsOnline.conflict')
    default: return t('modChanges.relation.unknown')
  }
}

function diagnosticRelation({ step, ownerLabel }: Edge) {
  const label = ownerLabel || t('modChanges.unnamedItem')
  const names = step.owner_version ? `${label} (${step.owner_version})` : label
  const kind = step.relation?.kind
  return t(kind === 'conflicts' ? 'modChanges.cause.conflicts_with' : kind === 'compatible'
    ? 'modChanges.diagnostic.compatibleWith' : 'modChanges.cause.required_by', { names })
}

function describeReason(reason: Reason) {
  return t(`modChanges.cause.${reason.kind}`, { names: listFormat.value.format(reason.related) })
}

function conditionParts(requirement: string | null | undefined, prerelease: boolean | null | undefined, conflict: boolean) {
  const parts: string[] = []
  if (requirement) parts.push(t(conflict ? 'modChanges.cause.conflictRange' : 'modChanges.cause.requiredRange', { range: requirement }))
  if (prerelease != null) parts.push(t(prerelease ? 'modChanges.cause.prereleaseOnly' : 'modChanges.cause.stableOnly'))
  return parts.join(' · ')
}

function diagnosticCondition({ step: { relation } }: Edge) {
  return conditionParts(formatVersionSpec(relation?.version), relation?.prerelease, relation?.kind === 'conflicts')
}

function describeCondition(reason: Reason) {
  const rule = conditionParts(reason.requirement, reason.prerelease, reason.kind === 'conflicts_with')
  if (!rule || (reason.kind !== 'conflicts_with' && reason.kind !== 'incompatible_with')) return rule
  const subject = reason.conditionOnRelated ? listFormat.value.format(reason.related) : t('modChanges.cause.thisItem')
  return t('modChanges.cause.condition', { subject, rule })
}
</script>

<template>
  <table v-if="rows.length" class="details-table" :aria-label="t('modChanges.detailsTable')">
    <thead>
      <tr>
        <th scope="col" class="details-operation">{{ t('modChanges.column.operation') }}</th>
        <th scope="col" class="details-category">{{ t('modChanges.column.category') }}</th>
        <th scope="col" class="plan-row-status">{{ t('modChanges.column.status') }}</th>
        <th scope="col">{{ t('modChanges.column.item') }}</th>
        <th scope="col" class="details-version">{{ t('modChanges.column.version') }}</th>
        <th scope="col">{{ t('modChanges.column.reason') }}</th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="row in rows" :key="row.key" :class="{ 'is-child': row.bundled || row.retired, 'diagnostic-start': row.diagnostic?.start }"
        :data-diagnostic-path="row.diagnostic?.path" :data-diagnostic-depth="row.diagnostic?.depth"
        :data-change-node="row.bundled || row.retired ? undefined : row.nodeId"
        :data-change-parent="row.bundled || row.retired ? row.nodeId : undefined" :data-change-retired="row.retired">
        <td class="details-operation">
          <NTag v-if="row.kind" size="small" :type="row.kind === 'retire' ? 'error' : row.kind === 'disable' ? 'warning' : 'info'">{{ t(`modChanges.action.${row.kind}`) }}</NTag>
        </td>
        <td class="details-category" :data-change-category="row.category">
          <span v-if="row.category" class="details-category-label">{{ t(`modChanges.category.${row.category}`) }}</span>
        </td>
        <td class="plan-row-status">
          <span v-if="row.status" class="row-status-icon" role="img" :aria-label="row.status.label"
            :title="row.status.label" :data-change-stage="row.status.stage" :data-change-icon="row.status.icon">
            <NSpin v-if="row.status.icon === 'spinner'" :size="16" aria-hidden="true" />
            <CloseOutline v-else-if="row.status.icon === 'error'" width="16" height="16" aria-hidden="true" />
            <CheckmarkOutline v-else-if="row.status.icon === 'check'" width="16" height="16" aria-hidden="true" />
            <ArrowUndoOutline v-else-if="row.status.icon === 'undo'" width="16" height="16" aria-hidden="true" />
            <EllipsisHorizontal v-else width="16" height="16" aria-hidden="true" />
          </span>
        </td>
        <th scope="row" class="details-item" :style="row.diagnostic?.depth ? { '--diagnostic-indent': `${Math.min(row.diagnostic.depth, 4) * 9}px` } : undefined">
          <span v-if="row.retired" class="details-member" :title="row.member?.path">{{ row.label || t('modChanges.unnamedItem') }}</span>
          <strong v-else-if="!row.bundled">{{ row.label || t('modChanges.unnamedItem') }}</strong>
          <span v-if="row.memberOnly || row.bundled" class="details-member" :title="row.member?.path">{{ row.member?.name || t('modChanges.unnamedPlugin') }}</span>
        </th>
        <td class="details-version">{{ row.version ? `${row.version.from ? `${row.version.from} → ` : ''}${row.version.to || '—'}` : row.diagnostic?.edge && !row.diagnostic.providers?.length ? '—' : '' }}</td>
        <td class="details-reason">
          <template v-if="row.diagnostic">
            <strong v-if="row.diagnostic.path" class="diagnostic-path">{{ t('modChanges.diagnostic.path', { number: row.diagnostic.path }) }}<template v-if="row.selected"> · {{ t('modChanges.detailsSelection') }}</template></strong>
            <template v-if="row.diagnostic.edge">
              {{ diagnosticRelation(row.diagnostic.edge) }}
              <span v-if="diagnosticCondition(row.diagnostic.edge)" class="reason-condition"> · {{ diagnosticCondition(row.diagnostic.edge) }}</span>
            </template>
            <ul v-if="row.diagnostic.providers && row.diagnostic.providers.length > 1" class="reason-list details-secondary">
              <li v-for="(provider, index) in row.diagnostic.providers" :key="index">
                {{ provider.label }} · {{ provider.version || '—' }}<template v-if="!provider.enabled"> · {{ t('common.state.disabled') }}</template>
              </li>
            </ul>
            <span v-else-if="row.diagnostic.providers?.[0]?.enabled === false && row.diagnostic.terminal?.state !== 'disabled'" class="details-secondary">{{ t('common.state.disabled') }}</span>
            <strong v-if="row.terminalReason" class="diagnostic-blocked">{{ row.terminalReason }}</strong>
          </template>
          <ul v-else-if="row.reasons?.length" class="reason-list">
            <li v-for="reason in row.reasons" :key="reason.key">
              {{ describeReason(reason) }}
              <span v-if="reason.requirement || reason.prerelease != null" class="reason-condition"> · {{ describeCondition(reason) }}</span>
            </li>
          </ul>
          <span v-else-if="row.retired">{{ t('modChanges.retireReason') }}</span>
          <span v-else-if="row.bundleReason">{{ t(`modChanges.bundle.${row.bundleReason}`) }}</span>
          <span v-else-if="row.selected">{{ t('modChanges.detailsSelection') }}</span>
          <template v-else>
            <span v-if="row.reason">{{ changeReasonText(row.reason) }}</span>
            <span v-if="row.causes?.length" class="details-secondary">{{ t('modChanges.causedBy', { names: listFormat.format(row.causes) }) }}</span>
          </template>
        </td>
      </tr>
    </tbody>
  </table>
  <p v-else-if="plan.status === 'ready'" class="details-empty">{{ t('modChanges.noChanges') }}</p>
</template>

<style scoped>
.details-table {
  width: 100%; border-spacing: 0; border: 1px solid var(--oc2-row-line); border-radius: 10px;
  overflow: hidden; color: var(--oc2-text-1); background: var(--oc2-row); font-size: 13px;
}
.details-table th, .details-table td { padding: 7px 12px; text-align: start; vertical-align: middle; line-height: 1.4; overflow-wrap: anywhere; }
.details-table thead th { background: var(--oc2-sunken); color: var(--oc2-text-2); font-size: 12px; font-weight: 600; }
.details-table tbody th, .details-table tbody td { border-top: 1px solid var(--oc2-row-line); }
.details-table tbody th { font-weight: 600; }
.details-table :is(.details-operation, .details-category, .plan-row-status) { width: 1%; white-space: nowrap; text-align: center; padding-inline: 8px; }
.details-category-label { color: var(--oc2-text-2); font-size: 12px; }
.details-version { font-variant-numeric: tabular-nums; color: var(--oc2-text-2); }
.details-member { color: var(--oc2-text-2); font-weight: 400; }
.details-item strong + .details-member { margin-inline-start: 8px; }
.details-table .is-child .details-item { padding-inline-start: 24px; }
.is-child .details-item, .is-child .details-reason { color: var(--oc2-text-2); }
.row-status-icon { display: inline-flex; align-items: center; justify-content: center; vertical-align: middle; min-height: 16px; color: var(--oc2-accent); }
.row-status-icon[data-change-icon='error'], .diagnostic-blocked { color: var(--oc2-danger, #e5484d); }
.row-status-icon:is([data-change-icon='dots'], [data-change-icon='undo']) { color: var(--oc2-text-2); }
.diagnostic-blocked { display: block; margin-top: 3px; }
.diagnostic-start th, .diagnostic-start td { background: var(--oc2-sunken); }
.diagnostic-path { display: block; }
.details-table:has(.diagnostic-start) :is(th, td) { padding-block: 5px; }
.details-table [data-diagnostic-depth] .details-item { padding-inline-start: calc(12px + var(--diagnostic-indent)); }
.details-table:has(.diagnostic-start) .details-version { width: 12%; }
.details-table:has(.diagnostic-start) .details-reason { width: 44%; }
.details-secondary, .reason-condition { color: var(--oc2-text-2); }
.details-secondary { display: block; font-size: 12px; }
.reason-list { list-style: none; padding: 0; margin: 0; }
.details-empty { margin: 0; }
.reason-list li + li { margin-top: 4px; }
@media (prefers-reduced-motion: reduce) {
  .row-status-icon :deep(.n-base-loading__container), .row-status-icon :deep(svg *) { animation: none !important; }
}
</style>
