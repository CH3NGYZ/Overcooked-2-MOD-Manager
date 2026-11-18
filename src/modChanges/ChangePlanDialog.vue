<script setup lang="ts">
import { computed, nextTick, useTemplateRef, watch } from 'vue'
import { NAlert, NButton, NProgress, NSpin } from 'naive-ui'
import AppModal from '../components/AppModal.vue'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { useModChanges } from './store'
import { useModChangesLifecycle } from './useModChangesLifecycle'
import { changeReasonKeys } from './messages'
import ChangePlanDetails from './ChangePlanDetails.vue'
import ChangePlanItem from './ChangePlanItem.vue'
import ModInstallNoteDialog from './ModInstallNoteDialog.vue'
import { changePlanRows, preparationCompletion } from './presentation'
import { activeDownloadProgress } from '../downloadTask'
import { appUpdateState } from '../appUpdateState'
function reasonText(reason: string) { return changeReasonKeys[reason] ? t(changeReasonKeys[reason]!) : translateBackendMessage(reason) }

const changes = useModChanges()
useModChangesLifecycle()
const state = changes.state
const installNotes = changes.installNotes
const plan = computed(() => state.value?.plan)
const preparation = changes.preparation
const currentProgress = computed(() => (state.value?.phase === 'preparing'
  || state.value?.phase === 'failed' && !plan.value?.prepared) ? preparation.value : null)
const rows = computed(() => changePlanRows(plan.value, currentProgress.value))
const detailsPlan = computed(() => plan.value && currentProgress.value ? { ...plan.value, actions: currentProgress.value.actions } : plan.value)
const completion = computed(() => preparationCompletion(preparation.value))
const waiting = computed(() => ['planning', 'preparing', 'executing'].includes(state.value?.phase ?? ''))
const canCancel = computed(() => !['preparing', 'executing'].includes(state.value?.phase ?? ''))
const labels = computed(() => new Map(plan.value?.nodes.map(node => [node.node_id, node.label]) ?? []))
const error = computed(() => state.value?.error ? backendMessageOf(state.value.error) : '')
const phase = computed(() => state.value?.phase === 'preparing' && preparation.value
  ? t(`modChanges.preparation.${preparation.value.stage}`)
  : t(`modChanges.phase.${state.value?.phase ?? 'planning'}`))
const downloadAbove = computed(() => state.value?.phase === 'preparing' && appUpdateState.stage === 'idle'
  && !!activeDownloadProgress.value)
const showDownloadFallback = computed(() => appUpdateState.stage === 'idle' && !downloadAbove.value
  && (changes.canCancelDownload.value || changes.cancellingDownload.value))
const statusElement = useTemplateRef<HTMLElement>('statusElement')
const buttons = useTemplateRef<HTMLElement>('buttons')
let restoreAfterDownload = false
watch([downloadAbove, () => state.value?.phase, () => installNotes.value.length], async ([above, workflowPhase, notes], _, onCleanup) => {
  if (above) { restoreAfterDownload = true; return }
  if (!workflowPhase || notes || appUpdateState.stage !== 'idle') { restoreAfterDownload = false; return }
  if (!restoreAfterDownload) return
  let stale = false
  onCleanup(() => { stale = true })
  await nextTick()
  if (stale || typeof document === 'undefined') return
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
  <AppModal v-else :show="!!state" :title="t('modChanges.title')" :z-index="2599" max-width="min(760px, 94%)"
    :closable="canCancel" :mask-closable="false" card-class="mod-changes-modal"
    @update:show="(show: boolean) => { if (!show) void changes.cancel() }">
    <div class="change-plan" data-mod-changes-plan>
      <p ref="statusElement" class="plan-phase" role="status" tabindex="-1" data-plan-status><NSpin v-if="waiting" size="small" />{{ phase }}</p>
      <NAlert v-if="state?.phase === 'reconfirming'" type="warning" :bordered="false">
        {{ t('modChanges.changed') }}
      </NAlert>
      <NAlert v-if="plan && plan.status !== 'ready'" type="error" :bordered="false">
        {{ t(plan.status === 'incomplete' ? 'modChanges.incomplete' : 'modChanges.unsatisfiable') }}
      </NAlert>
      <NAlert v-if="error" type="error" :bordered="false">{{ error }}</NAlert>
      <ul v-if="plan?.blockers.length" class="plan-notes">
        <li v-for="(blocker, index) in plan.blockers" :key="index">
          {{ reasonText(blocker.message || blocker.code) }}
          <span v-if="blocker.owner_id && labels.get(blocker.owner_id)"> · {{ labels.get(blocker.owner_id) }}</span>
          <span v-if="blocker.target_guid"> · {{ blocker.target_guid }}</span>
        </li>
      </ul>
      <div v-if="rows.length" class="plan-actions">
        <ChangePlanItem v-for="row in rows" :key="row.nodeId" :row="row" :processing="state?.phase === 'preparing'" />
      </div>
      <p v-else-if="plan?.status === 'ready'" class="plan-empty">{{ t('modChanges.noChanges') }}</p>
      <ChangePlanDetails v-if="detailsPlan" :plan="detailsPlan" />
      <p v-if="plan?.status === 'ready'" class="plan-footer-note">{{ t('modChanges.atomicNotice') }}</p>
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
          <NButton v-else data-plan-action="confirm" type="primary" :loading="waiting" :disabled="waiting || plan?.status !== 'ready'"
            @click="changes.confirm">{{ t(state?.phase === 'reconfirming' ? 'modChanges.reconfirm' : 'modChanges.confirm') }}</NButton>
        </div>
      </div>
    </template>
  </AppModal>
</template>

<style scoped>
.change-plan { display: flex; flex-direction: column; gap: 14px; min-width: 0; width: min(640px, calc(100vw - 100px)); max-width: 100%; }
.plan-progress p { margin: 8px 0 0; color: var(--oc2-text-3); font-size: 12px; }
.plan-bottom { width: 100%; min-width: 0; display: flex; flex-direction: column; gap: 12px; }
.plan-buttons { display: flex; justify-content: flex-end; align-items: center; gap: 8px; flex-wrap: wrap; }
.plan-phase { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin: 0; }
.progress-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; font-size: 12px; color: var(--oc2-text-3); margin-bottom: 8px; font-variant-numeric: tabular-nums; }
.plan-actions { display: flex; flex-direction: column; gap: 10px; }
.plan-footer-note { margin: 0; color: var(--oc2-text-3); font-size: 12px; line-height: 1.6; }
.plan-notes { margin: 0; padding-left: 20px; overflow-wrap: anywhere; }
.plan-empty { margin: 0; }
</style>
