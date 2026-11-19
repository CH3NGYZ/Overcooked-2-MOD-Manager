<script setup lang="ts">
import { computed, type DeepReadonly } from 'vue'
import { CheckmarkOutline, CloseOutline } from '@vicons/ionicons5'
import { NSpin, NTag } from 'naive-ui'
import { locale, t } from '../i18n'
import { changeReasonText } from './messages'
import { changePlanDetails } from './presentation'
import type { ChangeExecutionStage, ChangePlan, ChangePreparationProgress } from './types'

const { plan, progress, execution } = defineProps<{
  plan: DeepReadonly<ChangePlan>
  progress: DeepReadonly<ChangePreparationProgress> | null
  execution: ReadonlyMap<string, ChangeExecutionStage>
}>()

const details = computed(() => changePlanDetails(plan, progress))
const listFormat = computed(() => new Intl.ListFormat(locale.value, { style: 'short', type: 'conjunction' }))
type Row = ReturnType<typeof changePlanDetails>['rows'][number]
type Reason = NonNullable<Row['reasons']>[number]
const stageLabels = {
  applying: 'modChanges.apply.applying', processed: 'modChanges.apply.processed',
  applied: 'modChanges.apply.applied', failed: 'modChanges.apply.failed',
  downloading: 'modChanges.item.downloading', verifying: 'modChanges.item.verifying', preparationFailed: 'modChanges.item.failed',
} as const

const rows = computed(() => details.value.rows.map(row => {
  const applied = row.statusKey ? execution.get(row.statusKey) : undefined
  const prepared = row.preparationStage
  const stage = applied ?? (prepared === 'failed' ? 'preparationFailed' : prepared === 'downloading' || prepared === 'verifying' ? prepared : undefined)
  return { ...row, status: stage ? { stage, label: t(stageLabels[stage]),
    busy: stage === 'applying' || stage === 'downloading' || stage === 'verifying' } : undefined }
}))

function describeReason(reason: Reason) {
  return t(`modChanges.cause.${reason.kind}`, { names: listFormat.value.format(reason.related) })
}

function describeCondition(reason: Reason) {
  const parts: string[] = []
  if (reason.requirement) parts.push(t(reason.kind === 'conflicts_with' ? 'modChanges.cause.conflictRange' : 'modChanges.cause.requiredRange', { range: reason.requirement }))
  if (reason.prerelease != null) parts.push(t(reason.prerelease ? 'modChanges.cause.prereleaseOnly' : 'modChanges.cause.stableOnly'))
  const rule = parts.join(' · ')
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
        <th scope="col" class="plan-row-status">{{ t('modChanges.column.status') }}</th>
        <th scope="col">{{ t('modChanges.column.item') }}</th>
        <th scope="col" class="details-version">{{ t('modChanges.column.version') }}</th>
        <th scope="col">{{ t('modChanges.column.reason') }}</th>
      </tr>
    </thead>
    <tbody>
      <tr v-for="row in rows" :key="row.key" :class="{ 'is-child': row.bundled || row.retired }"
        :data-change-node="row.bundled || row.retired ? undefined : row.nodeId"
        :data-change-parent="row.bundled || row.retired ? row.nodeId : undefined" :data-change-retired="row.retired">
        <td class="details-operation">
          <NTag v-if="row.kind" size="small" :type="row.kind === 'retire' ? 'error' : row.kind === 'disable' ? 'warning' : 'info'">{{ t(`modChanges.action.${row.kind}`) }}</NTag>
        </td>
        <td class="plan-row-status">
          <span v-if="row.status" class="row-status-icon" role="img" :aria-label="row.status.label"
            :title="row.status.label" :data-change-stage="row.status.stage">
            <NSpin v-if="row.status.busy" :size="16" aria-hidden="true" />
            <CloseOutline v-else-if="row.status.stage === 'failed' || row.status.stage === 'preparationFailed'" width="16" height="16" aria-hidden="true" />
            <CheckmarkOutline v-else width="16" height="16" aria-hidden="true" />
          </span>
        </td>
        <th scope="row" class="details-item">
          <span v-if="row.retired" class="details-member" :title="row.member?.path">{{ row.label || t('modChanges.unnamedItem') }}</span>
          <strong v-else-if="!row.bundled">{{ row.label || t('modChanges.unnamedItem') }}</strong>
          <span v-if="row.memberOnly || row.bundled" class="details-member" :title="row.member?.path">{{ row.member?.name || t('modChanges.unnamedPlugin') }}</span>
        </th>
        <td class="details-version">{{ row.version ? `${row.version.from ? `${row.version.from} → ` : ''}${row.version.to || '—'}` : '' }}</td>
        <td class="details-reason">
          <ul v-if="row.reasons?.length" class="reason-list">
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
.details-operation, .details-table .plan-row-status { width: 1%; white-space: nowrap; }
.details-table .plan-row-status { text-align: center; padding-inline: 8px; }
.details-version { font-variant-numeric: tabular-nums; color: var(--oc2-text-2); }
.details-member { color: var(--oc2-text-2); font-weight: 400; }
.details-item strong + .details-member { margin-inline-start: 8px; }
.details-table .is-child .details-item { padding-inline-start: 24px; }
.is-child .details-item, .is-child .details-reason { color: var(--oc2-text-2); }
.row-status-icon { display: inline-flex; align-items: center; justify-content: center; vertical-align: middle; min-height: 16px; color: var(--oc2-accent); }
.row-status-icon:is([data-change-stage='failed'], [data-change-stage='preparationFailed']) { color: var(--oc2-danger, #e5484d); }
.details-secondary, .reason-condition { color: var(--oc2-text-2); }
.details-secondary { display: block; font-size: 12px; }
.reason-list { list-style: none; padding: 0; margin: 0; }
.details-empty { margin: 0; }
.reason-list li + li { margin-top: 4px; }
@media (prefers-reduced-motion: reduce) {
  .row-status-icon :deep(.n-base-loading__container), .row-status-icon :deep(svg *) { animation: none !important; }
}
</style>
