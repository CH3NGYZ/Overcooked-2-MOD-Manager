<script setup lang="ts">
import { computed, ref, type DeepReadonly } from 'vue'
import { NTag } from 'naive-ui'
import { locale, t } from '../i18n'
import { changeReasonText } from './messages'
import { changePlanDetails } from './presentation'
import type { ChangePlan, ChangePreparationProgress } from './types'

const { plan, progress, processing } = defineProps<{
  plan: DeepReadonly<ChangePlan>
  progress: DeepReadonly<ChangePreparationProgress> | null
  processing: boolean
}>()

const expanded = ref(false)
const hasDetails = computed(() => (progress?.actions ?? plan.actions).length > 0
  || plan.roots.some(root => root.label.trim()))
const details = computed(() => changePlanDetails(plan, progress))
const listFormat = computed(() => new Intl.ListFormat(locale.value, { style: 'short', type: 'conjunction' }))
type Reason = ReturnType<typeof changePlanDetails>['rows'][number]['reasons'][number]

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
  <details v-if="hasDetails" class="plan-details" :open="expanded" @toggle="expanded = ($event.currentTarget as HTMLDetailsElement).open">
    <summary>{{ t(expanded ? 'modChanges.hideDetails' : 'modChanges.details') }}</summary>
    <div v-if="expanded" class="details-content">
      <div v-if="details.roots.length" class="details-roots">
        <span>{{ t('modChanges.detailsSelection') }}</span>
        <div v-for="root in details.roots" :key="root.node_id" class="details-root">
          <strong>{{ root.label }}</strong><NTag v-if="root.tag" size="small">{{ root.tag }}</NTag>
        </div>
      </div>
      <table v-if="details.rows.length" class="details-table" :aria-label="t('modChanges.detailsTable')">
        <colgroup><col class="operation-col"><col class="item-col"><col class="version-col"><col></colgroup>
        <thead>
          <tr>
            <th scope="col">{{ t('modChanges.column.operation') }}</th>
            <th scope="col">{{ t('modChanges.column.item') }}</th>
            <th scope="col">{{ t('modChanges.column.version') }}</th>
            <th scope="col">{{ t('modChanges.column.reason') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in details.rows" :key="row.key">
            <td><NTag size="small" :type="row.kind === 'disable' ? 'warning' : 'info'">{{ t(`modChanges.action.${row.kind}`) }}</NTag></td>
            <th scope="row">
              <strong>{{ row.label || t('modChanges.unnamedItem') }}</strong>
              <span v-if="row.memberOnly" class="details-secondary" :title="row.member?.path">{{ row.member?.name || t('modChanges.unnamedPlugin') }}</span>
              <span v-if="row.showStage && (row.stage || processing)" class="details-secondary" role="status">{{ t(`modChanges.item.${row.stage ?? 'queued'}`) }}</span>
            </th>
            <td class="details-version">{{ row.version ? `${row.version.from ? `${row.version.from} → ` : ''}${row.version.to || '—'}` : '' }}</td>
            <td class="details-reason">
              <ul v-if="row.reasons.length" class="reason-list">
                <li v-for="reason in row.reasons" :key="reason.key">
                  {{ describeReason(reason) }}
                  <span v-if="reason.requirement || reason.prerelease != null" class="reason-condition">{{ describeCondition(reason) }}</span>
                </li>
              </ul>
              <template v-else>
                <span v-if="row.reason">{{ changeReasonText(row.reason) }}</span>
                <span v-if="row.causes.length" class="details-secondary">{{ t('modChanges.causedBy', { names: listFormat.format(row.causes) }) }}</span>
              </template>
            </td>
          </tr>
        </tbody>
      </table>
      <ul v-if="details.pending.length" class="details-pending">
        <li v-for="item in details.pending" :key="item.node_id">
          <strong>{{ item.label || t('modChanges.unnamedItem') }}</strong>
          <span v-if="item.tag">{{ item.tag }}</span>
          <span role="status">{{ t(`modChanges.item.${item.stage}`) }}</span>
        </li>
      </ul>
      <p v-if="!details.rows.length && !details.pending.length && plan.status === 'ready'" class="details-empty">{{ t('modChanges.noChanges') }}</p>
      <details v-for="pkg in details.packages" :key="pkg.nodeId" class="details-plugins">
        <summary>{{ t('modChanges.members', { name: pkg.label || t('modChanges.unnamedItem'), count: pkg.plugins.length }) }}</summary>
        <ul>
          <li v-for="plugin in pkg.plugins" :key="plugin.path">
            <span :title="plugin.path">{{ plugin.name }}</span>
            <NTag :type="plugin.enabled ? 'success' : 'default'" size="small">{{ t(plugin.enabled ? 'modChanges.memberState.enabled' : 'modChanges.memberState.disabled') }}</NTag>
          </li>
        </ul>
      </details>
    </div>
  </details>
  <slot v-if="!expanded || !hasDetails" />
</template>

<style scoped>
.plan-details { min-width: 0; font-size: 13px; }
.plan-details summary { width: fit-content; cursor: pointer; border-radius: 4px; color: var(--oc2-text-2); overflow-wrap: anywhere; }
.plan-details summary:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 4px; }
.details-content { display: flex; flex-direction: column; gap: 10px; padding-top: 10px; }
.details-roots, .details-root { display: flex; align-items: center; flex-wrap: wrap; gap: 6px 10px; }
.details-roots { color: var(--oc2-text-2); font-size: 12px; overflow-wrap: anywhere; }
.details-table {
  width: 100%; table-layout: fixed; border-spacing: 0; border: 1px solid var(--oc2-row-line);
  border-radius: 10px; overflow: hidden; color: var(--oc2-text-1); background: var(--oc2-row);
}
.operation-col { width: 16%; }
.item-col { width: 25%; }
.version-col { width: 20%; }
.details-table th, .details-table td { padding: 6px 8px; text-align: start; vertical-align: top; line-height: 1.45; overflow-wrap: anywhere; }
.details-table thead th { background: var(--oc2-sunken); color: var(--oc2-text-2); font-size: 12px; font-weight: 600; }
.details-table tbody th, .details-table tbody td { border-top: 1px solid var(--oc2-row-line); }
.details-table tbody th { font-weight: 600; }
.details-table td :deep(.n-tag) { max-width: 100%; height: auto; min-height: var(--n-height); white-space: normal; }
.details-version { font-variant-numeric: tabular-nums; color: var(--oc2-text-2); }
.details-secondary, .reason-condition { color: var(--oc2-text-2); }
.details-secondary, .reason-condition { display: block; font-size: 12px; font-weight: 400; }
.reason-list, .details-pending, .details-plugins ul { list-style: none; padding: 0; margin: 0; }
.details-empty { margin: 0; }
.reason-list li + li { margin-top: 6px; }
.details-plugins, .details-pending { color: var(--oc2-text-2); font-size: 12px; }
.details-plugins li, .details-pending li { display: flex; flex-wrap: wrap; gap: 4px 10px; align-items: center; overflow-wrap: anywhere; }
.details-plugins ul { margin-top: 6px; padding-inline-start: 12px; }
.details-plugins li + li { margin-top: 4px; }
</style>
