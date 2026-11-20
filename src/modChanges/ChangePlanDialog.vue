<script setup lang="ts">
import { computed, nextTick, useTemplateRef, watch } from 'vue'
import { NAlert, NButton, NProgress, NSpin } from 'naive-ui'
import AppModal from '../components/AppModal.vue'
import { locale, pickI18n, t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { useModChanges } from './store'
import { useModChangesLifecycle } from './useModChangesLifecycle'
import { changeReasonText } from './messages'
import ChangePlanDetails from './ChangePlanDetails.vue'
import ModInstallNoteDialog from './ModInstallNoteDialog.vue'
import { changePlanLabels, preparationCompletion } from './presentation'
import { activeDownloadProgress } from '../downloadTask'
import { appUpdateState } from '../appUpdateState'
import { isPackageAction } from './types'
import { isChangePlanLocked } from './workflow'
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
const needsPreparation = computed(() => !applyReview.value && plan.value?.actions.some(isPackageAction))
const confirmLabel = computed(() => applyReview.value === 'changed' && plan.value?.prepared ? 'modChanges.reconfirm'
  : needsPreparation.value ? 'modChanges.confirmPreparation' : 'modChanges.confirm')
const names = computed(() => plan.value && changePlanLabels(plan.value, currentProgress.value, pickI18n, locale.value))
const diagnostics = computed(() => {
  const blockers = plan.value?.blockers ?? []
  const notes = blockers.filter(blocker => plan.value?.status === 'ready' || !blocker.traces?.some(path => path.length))
  return { notes, hasPaths: notes.length !== blockers.length, truncated: blockers.some(blocker => blocker.traces_truncated) }
})
const error = computed(() => state.value?.error ? backendMessageOf(state.value.error) : '')
const phase = computed(() => {
  if (state.value?.phase === 'preparing' && preparation.value) return t(`modChanges.preparation.${preparation.value.stage}`)
  if (plan.value && plan.value.status !== 'ready') return t('modChanges.phase.failed')
  if (completedPreparation.value) return t('modChanges.preparation.ready')
  if (state.value?.phase === 'reconfirming' && applyReview.value === 'prepared') return t('modChanges.phase.applyReview')
  return t(`modChanges.phase.${state.value?.phase ?? 'planning'}`)
})
const downloadAbove = computed(() => state.value?.phase === 'preparing' && appUpdateState.stage === 'idle'
  && !!activeDownloadProgress.value)
const showDownloadFallback = computed(() => appUpdateState.stage === 'idle' && !downloadAbove.value
  && (changes.canCancelDownload.value || changes.cancellingDownload.value))
const statusElement = useTemplateRef<HTMLElement>('statusElement')
const buttons = useTemplateRef<HTMLElement>('buttons')
watch(statusElement, async element => {
  // The apply review is a new focus trap. Focus its heading instead of an
  // action, so a held Enter from the preparation button cannot approve writes.
  if (!element) return
  await nextTick()
  if (statusElement.value === element && state.value?.phase === 'reconfirming' && !installNotes.value.length && appUpdateState.stage === 'idle') {
    element.focus({ preventScroll: true })
  }
}, { flush: 'post' })
let restoreAfterDownload = false
watch([downloadAbove, () => state.value?.phase, () => installNotes.value.length], async ([above, workflowPhase, notes], _, onCleanup) => {
  if (above) { restoreAfterDownload = true; return }
  if (!workflowPhase || notes || appUpdateState.stage !== 'idle') { restoreAfterDownload = false; return }
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
  <ModInstallNoteDialog v-if="installNotes.length" :notes="installNotes"
    @continue="changes.acceptInstallNotes" @cancel="changes.cancel" />
  <AppModal v-else :key="applyReview ? 'apply-review' : 'prepare-review'"
    :show="!!state && !state.silent" :title="t(applyReview ? 'modChanges.applyTitle' : 'modChanges.title')" :z-index="2599" max-width="90%" max-height="90%"
    :closable="canCancel" :mask-closable="false" :auto-focus="!applyReview" card-class="mod-changes-modal"
    @update:show="(show: boolean) => { if (!show) void changes.cancel() }">
    <template #header-extra>
      <div ref="statusElement" class="plan-phase" role="status" tabindex="-1" data-plan-status>
        <NSpin v-if="working" size="small" class="plan-phase-spinner" aria-hidden="true" />
        <span>{{ phase }}</span>
      </div>
    </template>
    <div class="change-plan" data-mod-changes-plan :data-plan-phase="state?.phase" :data-plan-review="applyReview">
      <NAlert v-if="state?.phase === 'reconfirming' && applyReview === 'changed' && plan?.status === 'ready'" type="warning" :bordered="false">
        {{ t('modChanges.changed') }}
      </NAlert>
      <NAlert v-if="plan && plan.status !== 'ready'" type="error" :bordered="false">
        {{ t(plan.status === 'incomplete' ? 'modChanges.incomplete' : 'modChanges.unsatisfiable') }}
        <p v-if="diagnostics.hasPaths" class="diagnostic-note">{{ t('modChanges.diagnostic.note') }}</p>
        <p v-if="diagnostics.truncated" class="diagnostic-note">{{ t('modChanges.diagnostic.truncated') }}</p>
      </NAlert>
      <NAlert v-if="error" type="error" :bordered="false">{{ error }}</NAlert>
      <ul v-if="diagnostics.notes.length" class="plan-notes">
        <li v-for="(blocker, index) in diagnostics.notes" :key="index">
          {{ changeReasonText(blocker.message || blocker.code) }}
          <span v-if="blocker.owner_id && names?.nodes.get(blocker.owner_id)"> · {{ names.nodes.get(blocker.owner_id) }}</span>
          <span v-if="blocker.target_guid"> · {{ names?.dependency(blocker.target_guid) }}</span>
        </li>
      </ul>
      <ChangePlanDetails v-if="plan && names" :plan="plan" :progress="currentProgress" :execution="changes.execution" :names="names"
        :preparation-complete="completedPreparation" />
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
          <NButton v-if="state?.phase === 'failed'" data-plan-action="retry" type="primary" @click="changes.retry">{{ t('modChanges.retry') }}</NButton>
          <NButton v-else data-plan-action="confirm" type="primary" :loading="working" :disabled="working || completedPreparation || plan?.status !== 'ready'"
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
.diagnostic-note { margin: 3px 0 0; font-size: 12px; overflow-wrap: anywhere; }
</style>
