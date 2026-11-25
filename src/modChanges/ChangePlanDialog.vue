<script setup lang="ts">
import { computed, nextTick, useTemplateRef, watch } from 'vue'
import { NAlert, NButton, NProgress, NSpin } from 'naive-ui'
import AppModal from '../components/AppModal.vue'
import { locale, pickI18n, t } from '../i18n'
import { backendMessageOf, backendPayloadOf } from '../locale/backend'
import { useModChanges } from './store'
import { useModChangesLifecycle } from './useModChangesLifecycle'
import { changeReasonText } from './messages'
import ChangePlanDetails from './ChangePlanDetails.vue'
import ChangeEvidenceList from './ChangeEvidenceList.vue'
import ChangeRecoveryPanel from './ChangeRecoveryPanel.vue'
import { changeErrorEvidence, formatEvidenceRepository, providerEvidenceAdvice, providerRows } from './evidence'
import ModInstallNoteDialog from './ModInstallNoteDialog.vue'
import { changePlanLabels, preparationCompletion } from './presentation'
import { activeDownloadProgress } from '../downloadTask'
import { appUpdateBlocksOperations } from '../appUpdateState'
import { isPackageAction } from './types'
import { isChangePlanLocked, missingRecoveryApproval, needsRecoveryApproval, recoveryBlockedPlan } from './workflow'
import { openTransactionRecovery, showTransactionRecovery, transactionRecoveryUnavailable } from './transactionRecovery'
import { isTransactionRecoveryError } from './transactionRecoveryTypes'
import TransactionConflictConfirmation from './TransactionConflictConfirmation.vue'
const changes = useModChanges()
useModChangesLifecycle()
const state = changes.state
const installNotes = changes.installNotes
const plan = computed(() => state.value?.plan)
const preparation = changes.preparation
const currentProgress = computed(() => (state.value?.phase === 'preparing'
  || state.value?.phase === 'failed' && !plan.value?.prepared) ? preparation.value : null)
const completion = computed(() => preparationCompletion(preparation.value))
const working = computed(() => ['planning', 'preparing', 'executing'].includes(state.value?.phase ?? ''))
const completedPreparation = computed(() => state.value?.phase === 'prepared')
const canCancel = computed(() => !isChangePlanLocked(state.value?.phase))
const applyReview = computed(() => state.value?.applyReview)
const manifestOverwrite = computed(() => state.value?.manifestOverwrite)
const recoveryReview = computed(() => state.value?.phase === 'reconfirming' && plan.value?.prepared
  && plan.value.status === 'ready' && needsRecoveryApproval(plan.value))
const recoveryBlocked = computed(() => recoveryBlockedPlan(plan.value))
const recoveryProofMissing = computed(() => missingRecoveryApproval(plan.value))
const recoveryEntry = computed(() => needsRecoveryApproval(plan.value) || recoveryBlocked.value
  || isTransactionRecoveryError(backendPayloadOf(state.value?.error)?.tag))
const offerOverwrite = computed(() => state.value?.phase === 'failed' && !!manifestOverwrite.value && !recoveryEntry.value)
const overwriteReview = computed(() => !!applyReview.value && !!manifestOverwrite.value)
const sourceSwitch = computed(() => manifestOverwrite.value?.sourceSwitch)
const overwriteGroups = computed(() => {
  const verified = plan.value
  if (state.value?.phase !== 'reconfirming' || !applyReview.value || verified?.status !== 'ready' || !verified.prepared) return []
  const groups = new Map<string, Set<string>>()
  for (const { node_id: nodeId, path } of verified.untracked_overwrites ?? []) {
    const paths = groups.get(nodeId) ?? new Set<string>()
    paths.add(path)
    groups.set(nodeId, paths)
  }
  return [...groups]
})
const needsPreparation = computed(() => !applyReview.value && plan.value?.actions.some(isPackageAction))
const confirmLabel = computed(() => overwriteReview.value ? 'modChanges.manifestOverwrite.confirm'
  : applyReview.value === 'changed' && plan.value?.prepared ? 'modChanges.reconfirm'
  : needsPreparation.value ? 'modChanges.confirmPreparation' : 'modChanges.confirm')
const names = computed(() => plan.value && changePlanLabels(plan.value, currentProgress.value, pickI18n, locale.value))
const diagnostics = computed(() => {
  const blockers = plan.value?.blockers ?? []
  const notes = blockers.filter(blocker => plan.value?.status === 'ready' || !blocker.traces?.some(path => path.length))
  return {
    notes: notes.map(blocker => ({ blocker, evidence: changeErrorEvidence(blocker.message) })),
    hasPaths: notes.length !== blockers.length,
    truncated: blockers.some(blocker => blocker.traces_truncated),
  }
})
const notices = computed(() => [...new Set(plan.value?.notices ?? [])])
const error = computed(() => {
  if (state.value?.phase !== 'failed') return ''
  const reason = state.value.error
  return (reason == null ? '' : backendMessageOf(reason).trim()) || t('modChanges.failureWithoutDetails')
})
const failureEvidence = computed(() => changeErrorEvidence(state.value?.phase === 'failed' ? state.value.error : undefined))
const nextStep = computed(() => providerEvidenceAdvice(failureEvidence.value.providers))
const phase = computed(() => {
  if (state.value?.phase === 'preparing' && preparation.value) return t(`modChanges.preparation.${preparation.value.stage}`)
  if (plan.value && plan.value.status !== 'ready') return t('modChanges.phase.failed')
  if (completedPreparation.value) return t('modChanges.preparation.ready')
  if (state.value?.phase === 'reconfirming' && applyReview.value === 'prepared') return t('modChanges.phase.applyReview')
  return t(`modChanges.phase.${state.value?.phase ?? 'planning'}`)
})
const downloadAbove = computed(() => state.value?.phase === 'preparing' && !appUpdateBlocksOperations()
  && !!activeDownloadProgress.value)
const showDownloadFallback = computed(() => !appUpdateBlocksOperations() && !downloadAbove.value
  && (changes.canCancelDownload.value || changes.cancellingDownload.value))
const statusElement = useTemplateRef<HTMLElement>('statusElement')
const buttons = useTemplateRef<HTMLElement>('buttons')
watch(statusElement, async element => {
  // The apply review is a new focus trap. Focus its heading instead of an
  // action, so a held Enter from the preparation button cannot approve writes.
  if (!element) return
  await nextTick()
  if (statusElement.value === element && state.value?.phase === 'reconfirming' && !installNotes.value.length && !appUpdateBlocksOperations()) {
    element.focus({ preventScroll: true })
  }
}, { flush: 'post' })
let restoreAfterDownload = false
watch([downloadAbove, () => state.value?.phase, () => installNotes.value.length], async ([above, workflowPhase, notes], _, onCleanup) => {
  if (above) { restoreAfterDownload = true; return }
  if (!workflowPhase || notes || appUpdateBlocksOperations()) { restoreAfterDownload = false; return }
  if (!restoreAfterDownload) return
  let stale = false
  onCleanup(() => { stale = true })
  await nextTick()
  if (stale || typeof document === 'undefined') return
  if (workflowPhase === 'reconfirming') { restoreAfterDownload = false; return }
  const focused = document.activeElement
  const ownDialog = statusElement.value?.closest('.mod-changes-modal')
  // Do not take focus away from a new dialog or an intentional move to the title bar.
  if (focused !== document.body && !ownDialog?.contains(focused) && !focused?.closest('.dl-card')) {
    restoreAfterDownload = false
    return
  }
  const button = buttons.value?.querySelector<HTMLElement>('[data-plan-action="retry"], [data-plan-action="confirm"]:not(:disabled)')
  const target = button ?? statusElement.value
  target?.focus({ preventScroll: true })
  if (button) restoreAfterDownload = false
}, { flush: 'post' })
</script>

<template>
  <ModInstallNoteDialog v-if="installNotes.length && plan" :key="plan.plan_id" :plan-id="plan.plan_id"
    :notes="installNotes" :apply-on-read="!!state?.noteOnlyReview"
    @continue="changes.acceptInstallNotes" @cancel="changes.cancel" />
  <AppModal v-else :key="applyReview ? 'apply-review' : 'prepare-review'"
    :show="!!state && !state.silent" :title="t(overwriteReview ? 'modChanges.manifestOverwrite.confirm' : applyReview ? 'modChanges.applyTitle' : 'modChanges.title')" :z-index="2599" max-width="90%" max-height="90%"
    :closable="canCancel" :mask-closable="false" :auto-focus="!applyReview" card-class="mod-changes-modal"
    @update:show="(show: boolean) => { if (!show) void changes.cancel() }">
    <template #header-extra>
      <div ref="statusElement" class="plan-phase" role="status" tabindex="-1" data-plan-status>
        <NSpin v-if="working" size="small" class="plan-phase-spinner" aria-hidden="true" />
        <span>{{ phase }}</span>
      </div>
    </template>
    <div class="change-plan" data-mod-changes-plan :data-plan-phase="state?.phase" :data-plan-review="applyReview">
      <NAlert v-for="notice in notices" :key="notice" type="warning" :bordered="false" data-plan-notice>
        <span class="plan-message">{{ backendMessageOf(notice) }}</span>
      </NAlert>
      <NAlert v-if="state?.phase === 'reconfirming' && applyReview === 'changed' && plan?.status === 'ready'" type="warning" :bordered="false">
        {{ t('modChanges.changed') }}
      </NAlert>
      <NAlert v-if="overwriteGroups.length" type="warning" :bordered="false"
        :title="t('modChanges.overwrite.title')" data-untracked-overwrites>
        {{ t('modChanges.overwrite.description') }}
        <ul class="overwrite-groups" role="list">
          <li v-for="[nodeId, paths] in overwriteGroups" :key="nodeId">
            <strong>{{ names?.nodes.get(nodeId) ?? t('modChanges.unnamedItem') }}</strong>
            <ChangeEvidenceList :rows="Array.from(paths, path => ({ path }))" />
          </li>
        </ul>
      </NAlert>
      <NAlert v-if="plan && plan.status !== 'ready'" type="error" :bordered="false">
        {{ t(plan.status === 'incomplete' ? 'modChanges.incomplete' : 'modChanges.unsatisfiable') }}
        <p v-if="diagnostics.hasPaths" class="diagnostic-note">{{ t('modChanges.diagnostic.note') }}</p>
        <p v-if="diagnostics.truncated" class="diagnostic-note">{{ t('modChanges.diagnostic.truncated') }}</p>
      </NAlert>
      <NAlert v-if="error && !offerOverwrite" type="error" :bordered="false">
        <span class="plan-error plan-message">{{ error }}</span>
        <p v-if="nextStep" class="diagnostic-note">{{ t(nextStep) }}</p>
        <ChangeEvidenceList :rows="failureEvidence.rows" :guid="failureEvidence.providers?.guid"
          :truncated="failureEvidence.providers?.truncated" :names="names?.nodes" />
      </NAlert>
      <ChangeRecoveryPanel v-if="state?.phase === 'failed' && state.error === 'levelSet.recoveryRequired'" />
      <NAlert v-if="recoveryEntry" type="warning" :bordered="false" data-plan-recovery-conflict>
        <template v-if="plan?.recovery_conflicts?.length">
          <p class="diagnostic-note">{{ t('modChanges.transactionRecovery.conflictDescription') }}</p>
          <section v-for="conflict in plan.recovery_conflicts" :key="conflict.transaction_id" class="plan-message">
            <code>{{ conflict.transaction_id }}</code>
            <ChangeEvidenceList :rows="conflict.paths.map(path => ({ path }))" />
          </section>
        </template>
        <p v-if="recoveryProofMissing" class="diagnostic-note">{{ t('modChanges.recoveryConflictChanged') }}</p>
        <NButton size="small" secondary :disabled="transactionRecoveryUnavailable()" data-plan-action="open-transaction-recovery"
          @click="openTransactionRecovery">{{ t('modChanges.transactionRecovery.title') }}</NButton>
      </NAlert>
      <NAlert v-if="manifestOverwrite && (offerOverwrite || overwriteReview)" type="warning" :bordered="false" data-manifest-overwrite
        :title="t(sourceSwitch ? 'modChanges.manifestOverwrite.sourceSwitch' : offerOverwrite ? 'modChanges.manifestOverwrite.conflict' : 'modChanges.manifestOverwrite.title')">
        <p v-if="offerOverwrite" class="manifest-copy plan-message">{{ t('modChanges.manifestOverwrite.recheck') }}</p>
        <template v-if="sourceSwitch">
          <p class="manifest-copy plan-message">{{ t(sourceSwitch.installedPath ? 'modChanges.manifestOverwrite.replaceInstalled' : 'modChanges.manifestOverwrite.installMissing') }}</p>
          <dl class="source-switch-evidence plan-message">
            <dt>{{ t('modChanges.manifestOverwrite.previousSource') }}</dt>
            <dd>{{ formatEvidenceRepository(sourceSwitch.previousSource) }} · {{ sourceSwitch.previousSource.tag }}</dd>
            <dt>{{ t('modChanges.manifestOverwrite.nextSource') }}</dt>
            <dd>{{ formatEvidenceRepository(sourceSwitch.nextSource) }} · {{ sourceSwitch.nextSource.tag }}</dd>
            <dt>{{ t('modChanges.manifestOverwrite.title') }}</dt>
            <dd><ChangeEvidenceList :rows="[{ path: manifestOverwrite.manifestPath }]" /></dd>
            <template v-if="sourceSwitch.installedPath">
              <dt>{{ t('modChanges.manifestOverwrite.installedPath') }}</dt>
              <dd><ChangeEvidenceList :rows="[{ path: sourceSwitch.installedPath }]" /></dd>
            </template>
            <dt>{{ t('modChanges.manifestOverwrite.targetPath') }}</dt>
            <dd><ChangeEvidenceList :rows="[{ path: sourceSwitch.targetPath }]" /></dd>
          </dl>
        </template>
        <template v-else>
          <ChangeEvidenceList :rows="[{ path: manifestOverwrite.manifestPath }]" />
          <p class="manifest-copy plan-message">{{ t('modChanges.manifestOverwrite.description') }}</p>
          <template v-if="manifestOverwrite.recordedPaths.length">
            <div>{{ t('modChanges.manifestOverwrite.paths') }}</div>
            <ChangeEvidenceList :rows="manifestOverwrite.recordedPaths.map(path => ({ path }))" />
          </template>
        </template>
      </NAlert>
      <ul v-if="diagnostics.notes.length" class="plan-notes">
        <li v-for="({ blocker, evidence }, index) in diagnostics.notes" :key="index">
          <span class="plan-message">{{ changeReasonText(blocker.message || blocker.code) }}</span>
          <span v-if="blocker.owner_id && names?.nodes.get(blocker.owner_id)"> · {{ names.nodes.get(blocker.owner_id) }}</span>
          <span v-if="blocker.target_guid"> · {{ names?.dependency(blocker.target_guid) }}</span>
          <ChangeEvidenceList :rows="evidence.rows" :guid="evidence.providers?.guid"
            :truncated="evidence.providers?.truncated" :names="names?.nodes" />
        </li>
      </ul>
      <ChangePlanDetails v-if="plan && names" :plan="plan" :progress="currentProgress" :execution="changes.execution" :names="names"
        :preparation-complete="completedPreparation" />
      <details v-if="plan?.provider_choices?.length" class="provider-choices" :open="plan.status !== 'ready'" data-provider-choices>
        <summary>{{ t('modChanges.evidence.compare') }}</summary>
        <p v-if="plan.status !== 'ready'" class="diagnostic-note">{{ t('modChanges.evidence.checkPlanned') }}</p>
        <ChangeEvidenceList v-for="group in plan.provider_choices" :key="group.guid" :rows="providerRows(group)"
          :guid="group.guid" :truncated="group.truncated" :names="names?.nodes" />
      </details>
    </div>
    <template #footer>
      <div class="plan-bottom">
        <section v-if="state?.phase === 'planning' || state?.phase === 'preparing'" class="plan-progress" role="status" aria-busy="true">
          <template v-if="state.phase === 'preparing' && completion.total">
            <div class="progress-heading"><span>{{ t('modChanges.progress.packages', { completed: completion.completed, total: completion.total }) }}</span><span>{{ completion.percentage }}%</span></div>
            <NProgress type="line" :percentage="completion.percentage ?? 0" :show-indicator="false" :height="6" />
          </template>
          <template v-else>
            <NProgress type="line" :percentage="100" :show-indicator="false" :height="6" processing />
            <p>{{ t(state.phase === 'planning' ? 'modChanges.progress.planning' : 'modChanges.progress.preparing') }}</p>
          </template>
        </section>
        <div ref="buttons" class="plan-buttons">
          <NButton v-if="showDownloadFallback" :loading="changes.cancellingDownload.value"
            :disabled="!changes.canCancelDownload.value" @click="changes.cancelPreparationDownload">{{ t('common.cancelDownload') }}</NButton>
          <NButton data-plan-action="cancel" :disabled="!canCancel" @click="changes.cancel">{{ t('common.action.cancel') }}</NButton>
          <NButton v-if="offerOverwrite" data-plan-action="overwrite-manifest" type="warning"
            @click="changes.forceOverwriteManifest">{{ t('modChanges.manifestOverwrite.action') }}</NButton>
          <NButton v-else-if="state?.phase === 'failed' || recoveryBlocked || recoveryProofMissing" data-plan-action="retry" type="primary"
            @click="changes.retry">{{ t('modChanges.retry') }}</NButton>
          <TransactionConflictConfirmation v-else-if="recoveryReview && plan && !showTransactionRecovery"
            :plan="plan" @approve="changes.confirmRecovery" />
          <NButton v-else data-plan-action="confirm" :type="overwriteReview ? 'warning' : 'primary'" :loading="working" :disabled="working || completedPreparation || plan?.status !== 'ready' || recoveryReview"
            @click="changes.confirm">{{ t(confirmLabel) }}</NButton>
        </div>
      </div>
    </template>
  </AppModal>
</template>

<style scoped>
.change-plan { display: flex; flex-direction: column; gap: 12px; min-width: 0; max-width: 100%; }
.plan-progress p { margin: 8px 0 0; color: var(--oc2-text-3); font-size: 12px; }
.plan-bottom { width: 100%; min-width: 0; display: flex; flex-direction: column; gap: 12px; }
.plan-buttons { display: flex; justify-content: flex-end; align-items: center; gap: 8px; flex-wrap: wrap; }
:global(.mod-changes-modal .n-card-header__extra) { min-width: 0; max-width: min(60%, 360px); margin-inline-start: 12px; }
.plan-phase { display: flex; align-items: center; gap: 8px; min-width: 0; font-size: 12px; line-height: 1.4; color: var(--oc2-text-2); text-align: end; overflow-wrap: anywhere; }
.plan-phase-spinner { flex: none; }
.progress-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; font-size: 12px; color: var(--oc2-text-3); margin-bottom: 8px; font-variant-numeric: tabular-nums; }
.plan-notes { margin: 0; padding-left: 20px; overflow-wrap: anywhere; }
.plan-error { display: block; }
.plan-message { white-space: pre-wrap; overflow-wrap: anywhere; }
.overwrite-groups { margin: 8px 0 0; padding: 0; list-style: none; overflow-wrap: anywhere; }
.overwrite-groups > li + li { margin-block-start: 8px; }
.manifest-copy { margin: 4px 0 6px; }
.source-switch-evidence { margin: 8px 0 0; }
.source-switch-evidence dt { font-weight: 600; }
.source-switch-evidence dd { margin: 0 0 6px; }
.provider-choices { min-width: 0; }
.provider-choices summary { cursor: pointer; }
.diagnostic-note { margin: 3px 0 0; font-size: 12px; overflow-wrap: anywhere; }
</style>
