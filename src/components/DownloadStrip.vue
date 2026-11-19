<script setup lang="ts">
import { computed, nextTick, onUnmounted, shallowRef, useTemplateRef, watch } from 'vue'
import { NButton, NProgress } from 'naive-ui'
import { t } from '../i18n'
import { api, watchDownloadProgress } from '../services/tauri'
import { translateBackendMessage } from '../locale/backend'
import { activeDownloadProgress, downloadTaskState, dismissDownloadFailure, retryDownloadTask, updateDownloadProgress, cancelDownloadTask } from '../downloadTask'
import { downloadAmount, downloadPercent, downloadRace, downloadRate, downloadRoute } from '../downloadPresentation'
import { appUpdateState } from '../appUpdateState'
import AppModal from './AppModal.vue'
import DownloadVolumes from './DownloadVolumes.vue'

const progress = activeDownloadProgress
const running = computed(() => downloadTaskState.running)
const failure = computed(() => downloadTaskState.failure)
const canCancel = computed(() => appUpdateState.stage === 'idle')
const visible = computed(() => canCancel.value
  ? !!(failure.value || progress.value)
  : appUpdateState.stage === 'installing' && !failure.value && !!progress.value)

const label = computed(() => translateBackendMessage(failure.value?.label ?? progress.value?.label ?? ''))
const title = computed(() => failure.value ? t('tools.downloadFailed') : label.value || t('tools.downloadProgress'))
const percent = computed(() => progress.value ? downloadPercent(progress.value) : null)
const stageText = computed(() => {
  switch (progress.value?.phase) {
    case 'connecting': return t('tools.connectingToDownloadSource')
    case 'switching': return t('tools.switchingToAFasterSource')
    case 'downloading': return ''
    case 'merging': return t('tools.mergingVolumes')
    default: return t('tools.preparing2')
  }
})
const route = computed(() => progress.value ? downloadRoute(progress.value) : '')
const race = computed(() => progress.value ? downloadRace(progress.value) : '')
const rate = computed(() => progress.value ? downloadRate(progress.value) : '')
const volumes = computed(() => downloadTaskState.progress?.volumes)
const volumeKey = computed(() => `${running.value?.id ?? failure.value?.id}:${label.value}`)

const titleElement = useTemplateRef<HTMLElement>('titleElement')
const actions = useTemplateRef<HTMLElement>('actions')

// NModal owns the focus stack. Teleported children can mount before its focus
// trap is active; if opening left focus outside, retry after the enter transition.
function focusActions(keepExisting = false) {
  const heading = titleElement.value
  if (!visible.value || !heading) return
  if (keepExisting && heading.closest('.dl-card')?.contains(document.activeElement)) return
  const target = actions.value?.querySelector<HTMLElement>('[data-download-action]') ?? heading
  target.focus({ preventScroll: true })
}

// Failure and retry replace the focused controls in place when the next task
// starts before the overlay can leave. Progress events do not move focus.
watch(() => failure.value?.id, async id => {
  await nextTick()
  if (id === failure.value?.id) focusActions()
}, { flush: 'post' })

let unlisten: (() => void) | null = null
let disposed = false
// Private callers update this same model with task tokens; App owns the sole event listener.
void watchDownloadProgress(value => { if (!disposed) updateDownloadProgress(value) })
  .then(un => { if (disposed) un(); else unlisten = un })
  .catch(() => undefined)
onUnmounted(() => { disposed = true; unlisten?.() })

const cancelling = shallowRef<number | null>(null)
async function cancel() {
  if (!canCancel.value) return
  const task = running.value
  if (!task || task.id === cancelling.value) return
  if (!task.cancel) cancelDownloadTask(task.id)
  cancelling.value = task.id
  try {
    if (task.cancel) await task.cancel()
    else await api.cancelDownload()
  } catch { /* The native download may reject cancellation; retain any live caller progress. */ }
  finally { if (cancelling.value === task.id) cancelling.value = null }
}
</script>

<template>
  <AppModal :show="visible" :closable="false" :block-scroll="false" :z-index="canCancel ? 2600 : 6200"
    :card-class="`dl-card${failure ? ' dl-card-error' : ''}${canCancel ? '' : ' dl-card-update'}`"
    max-width="min(480px, 94%)" aria-labelledby="download-modal-title" @after-enter="focusActions(true)">
    <template #header>
      <h2 id="download-modal-title" ref="titleElement" class="dl-title" :tabindex="canCancel ? -1 : 0" :title="title">{{ title }}</h2>
    </template>

    <template v-if="failure">
      <div class="dl-error" role="alert">
        <p v-if="label" class="dl-error-label">{{ label }}</p>
        <p class="dl-error-reason">{{ failure.message }}</p>
      </div>
    </template>
    <div v-else-if="progress" class="dl-body">
      <div class="dl-progress-row" role="progressbar" :aria-label="t('tools.downloadProgress')"
        :aria-valuenow="percent === null ? undefined : Math.round(percent)" aria-valuemin="0" aria-valuemax="100">
        <!-- Naive always exposes a value, including when processing. The wrapper owns accessible progress. -->
        <NProgress type="line" :percentage="percent ?? 100" :processing="percent === null" :show-indicator="false"
          :height="6" :border-radius="3" aria-hidden="true" class="dl-bar" />
        <span v-if="percent !== null" class="dl-percent">{{ percent.toFixed(1) }}%</span>
      </div>
      <div class="dl-stats">
        <div class="dl-amount-rate">
          <span class="dl-amount">{{ downloadAmount(progress) }}</span>
          <span v-if="rate" class="dl-rate">{{ rate }}</span>
        </div>
        <span v-if="stageText" class="dl-stage" role="status">{{ stageText }}</span>
        <div v-if="route || race" class="dl-state-route">
          <span v-if="route" class="dl-route" :title="route">{{ route }}</span>
          <span v-if="race" class="dl-race">{{ race }}</span>
        </div>
      </div>
    </div>

    <DownloadVolumes v-if="volumes?.length" :key="volumeKey" :volumes="volumes" />

    <template v-if="canCancel" #footer>
      <div ref="actions" class="dl-actions">
        <template v-if="failure">
          <NButton data-download-action="retry" tabindex="0" type="primary" size="small" @click="retryDownloadTask">{{ t('common.action.retry') }}</NButton>
          <NButton data-download-action="close" tabindex="0" secondary size="small" @click="dismissDownloadFailure">{{ t('nav.close') }}</NButton>
        </template>
        <NButton v-else data-download-action="cancel" tabindex="0" secondary size="small"
          :loading="running !== null && cancelling === running.id" @click="cancel">{{ t('common.cancelDownload') }}</NButton>
      </div>
    </template>
  </AppModal>
</template>

<style>
.dl-card.n-card {
  --dl-gutter: 18px;
  --n-padding-left: var(--dl-gutter);
  --n-padding-right: var(--dl-gutter);
  --n-padding-top: 14px;
  --n-padding-bottom: 14px;
}
.dl-card .n-card-header { align-items: flex-start; padding: 14px var(--dl-gutter) 10px; }
.dl-card .n-card-header__main { min-width: 0; }
.dl-card .n-card-content { padding: 0 var(--dl-gutter) 14px; }
.dl-card .app-modal-scroll { gap: 9px; }
.dl-card .app-modal-footer { margin-top: 8px; padding-top: 9px; }
</style>

<style scoped>
.dl-title { display: -webkit-box; -webkit-box-orient: vertical; -webkit-line-clamp: 2; overflow: hidden; margin: 0; font-size: 16px; line-height: 1.35; font-weight: 650; color: var(--oc2-text-1); overflow-wrap: anywhere; }
.dl-body { display: grid; gap: 8px; min-width: 0; }
.dl-progress-row { display: flex; align-items: center; gap: 9px; min-height: 16px; }
.dl-bar { flex: 1; min-width: 0; }
.dl-percent { flex: none; min-width: 48px; text-align: right; font-size: 12px; font-variant-numeric: tabular-nums; color: var(--oc2-text-2); }
.dl-stats { display: grid; gap: 4px; font-size: 12px; color: var(--oc2-text-2); font-variant-numeric: tabular-nums; }
.dl-amount-rate, .dl-state-route { display: flex; align-items: baseline; gap: 10px; min-width: 0; }
.dl-rate, .dl-race { flex: none; margin-inline-start: auto; white-space: nowrap; }
.dl-rate { font-weight: 650; }
.dl-stage { font-weight: 550; }
.dl-route { color: var(--oc2-text-3); min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dl-race { color: var(--oc2-accent); }
.dl-error { overflow-wrap: anywhere; }
.dl-error-label { margin: 0; font-size: 13px; font-weight: 600; color: var(--oc2-text-2); }
.dl-error-reason { margin: 4px 0 0; font-size: 13px; line-height: 1.45; color: var(--oc2-text-1); white-space: pre-wrap; }
.dl-actions { display: flex; justify-content: flex-end; gap: 8px; flex-wrap: wrap; }

@media (prefers-reduced-motion: reduce) {
  .dl-bar :deep(.n-progress-graph-line-fill),
  .dl-bar :deep(.n-progress-graph-line-fill::after) { animation: none; transition: none; }
}
</style>
