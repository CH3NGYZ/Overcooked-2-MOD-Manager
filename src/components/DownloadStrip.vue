<script setup lang="ts">
import { computed, nextTick, shallowRef, useTemplateRef, watch } from 'vue'
import { NProgress } from 'naive-ui'
import { AlertCircleOutline, CloseOutline, DownloadOutline, RefreshOutline, ServerOutline, SpeedometerOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { api } from '../services/tauri'
import { translateBackendMessage } from '../locale/backend'
import { activeDownloadProgress, downloadTaskState, dismissDownloadFailure, retryDownloadTask, cancelDownloadTask } from '../downloadTask'
import { downloadAmount, downloadPercent, downloadRate, downloadRoute } from '../downloadPresentation'
import { appUpdateBlocksOperations } from '../appUpdateState'
import AppModal from './AppModal.vue'
import DownloadRace from './DownloadRace.vue'
import DownloadVolumes from './DownloadVolumes.vue'
import HeaderIconAction from './HeaderIconAction.vue'

const progress = activeDownloadProgress
const running = computed(() => downloadTaskState.running)
const failure = computed(() => downloadTaskState.failure)
const visible = computed(() => !appUpdateBlocksOperations() && !!(failure.value || progress.value))

const label = computed(() => translateBackendMessage(failure.value?.label ?? progress.value?.label ?? ''))
const fileName = computed(() => progress.value?.file_name || label.value)
const percent = computed(() => progress.value ? downloadPercent(progress.value) : null)
const percentText = computed(() => percent.value?.toFixed(1))
const title = computed(() => {
  if (failure.value) return t('tools.downloadFailed')
  switch (progress.value?.phase) {
    case 'connecting': return t('tools.connectingToDownloadSource')
    case 'switching': return t('tools.downloadRace.switching')
    case 'verifying': return t('tools.downloadRace.verifying')
    case 'locked_failed': return t('tools.downloadRace.lockedFailed')
    case 'merging': return t('tools.mergingVolumes')
    default: return t('tools.downloading')
  }
})
const route = computed(() => progress.value ? downloadRoute(progress.value) : '')
const rate = computed(() => progress.value ? downloadRate(progress.value) : '')
const volumes = computed(() => downloadTaskState.progress?.volumes)
const volumeKey = computed(() => `${running.value?.id ?? failure.value?.id}:${fileName.value}`)
const cancelLabel = computed(() => t('common.cancelDownload'))

const titleElement = useTemplateRef<HTMLElement>('titleElement')

// NModal owns the focus stack. Teleported children can mount before its focus
// trap is active; if opening left focus outside, retry after the enter transition.
function focusActions(keepExisting = false) {
  const heading = titleElement.value
  if (!visible.value || !heading) return
  const card = heading.closest('.dl-card')
  if (keepExisting && card?.contains(document.activeElement)) return
  const target = card?.querySelector<HTMLElement>('[data-download-action]') ?? heading
  target.focus({ preventScroll: true })
}

// Failure and retry replace the focused controls in place when the next task
// starts before the overlay can leave. Progress events do not move focus.
watch(() => failure.value?.id, async id => {
  await nextTick()
  if (id === failure.value?.id) focusActions()
}, { flush: 'post' })

const cancelling = shallowRef<number | null>(null)
const cancelPending = computed(() => running.value !== null && cancelling.value === running.value.id)
async function cancel() {
  if (appUpdateBlocksOperations()) return
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
  <AppModal :show="visible" :closable="false" :block-scroll="false" :z-index="2600"
    :card-class="`dl-card${failure ? ' dl-card-error' : ''}`"
    style="width: 480px" max-width="94%" aria-labelledby="download-modal-title" @after-enter="focusActions(true)">
    <template #header>
      <div class="dl-heading">
        <span class="dl-heading-icon" aria-hidden="true">
          <AlertCircleOutline v-if="failure" width="22" height="22" />
          <DownloadOutline v-else width="22" height="22" />
        </span>
        <div class="dl-heading-text">
          <h2 id="download-modal-title" ref="titleElement" class="dl-title" tabindex="-1" :title="title">
            <span role="status">{{ title }}</span>
          </h2>
          <span v-if="!failure && progress && fileName" id="download-modal-file" class="dl-file" :title="fileName">{{ fileName }}</span>
        </div>
        <strong v-if="!failure && percentText !== undefined" class="dl-percent" aria-hidden="true">{{ percentText }}<span>%</span></strong>
        <HeaderIconAction v-if="!failure && progress" class="dl-cancel" data-download-action="cancel"
          quaternary circle hover-only :label="cancelLabel"
          :loading="cancelPending" :aria-busy="cancelPending" @click="cancel">
          <CloseOutline width="18" height="18" aria-hidden="true" />
        </HeaderIconAction>
      </div>
    </template>

    <template v-if="failure">
      <div class="dl-error" role="alert">
        <p v-if="label" class="dl-error-label">{{ label }}</p>
        <p class="dl-error-reason">{{ failure.message }}</p>
      </div>
    </template>
    <div v-else-if="progress" class="dl-body">
      <div class="dl-progress-row" role="progressbar" :aria-label="t('tools.downloadProgress')"
        :aria-describedby="fileName ? 'download-modal-file' : undefined"
        :aria-valuenow="percentText" aria-valuemin="0" aria-valuemax="100">
        <!-- Naive always exposes a value, including when processing. The wrapper owns accessible progress. -->
        <NProgress type="line" :percentage="percent ?? (progress.phase === 'locked_failed' ? 0 : 100)"
          :processing="percent === null && progress.phase !== 'locked_failed'" :show-indicator="false"
          :height="8" :border-radius="4" aria-hidden="true" class="dl-bar" />
      </div>
      <div class="dl-stats">
        <div class="dl-amount-rate">
          <span class="dl-amount"><ServerOutline width="15" height="15" aria-hidden="true" />{{ downloadAmount(progress) }}</span>
          <span v-if="rate" class="dl-rate"><SpeedometerOutline width="15" height="15" aria-hidden="true" />{{ rate }}</span>
        </div>
        <div v-if="route && !progress.race" class="dl-state-route" :class="{ 'is-transferring': progress.phase === 'downloading' && progress.speed_bytes > 0 }">
          <span v-if="route" class="dl-route" :title="route">{{ route }}</span>
        </div>
      </div>
    </div>

    <DownloadRace v-if="progress?.race" :key="`${running?.id}:${progress.race.id}`" :race="progress.race" interactive />
    <DownloadVolumes v-if="volumes?.length" :key="volumeKey" :volumes="volumes" />

    <template v-if="failure" #footer>
      <div class="dl-actions">
        <HeaderIconAction data-download-action="retry" type="primary" circle hover-only :label="t('common.action.retry')" @click="retryDownloadTask">
          <RefreshOutline aria-hidden="true" />
        </HeaderIconAction>
        <HeaderIconAction data-download-action="close" secondary circle hover-only :label="t('nav.close')" @click="dismissDownloadFailure">
          <CloseOutline aria-hidden="true" />
        </HeaderIconAction>
      </div>
    </template>
  </AppModal>
</template>

<style>
.dl-card.n-card {
  --dl-gutter: 24px;
  --n-padding-left: var(--dl-gutter);
  --n-padding-right: var(--dl-gutter);
  --n-padding-top: 16px;
  --n-padding-bottom: 16px;
  border-radius: 18px;
}
.dl-card .n-card-header { padding: 16px var(--dl-gutter) 8px; }
.dl-card .n-card-header__main { min-width: 0; }
/* Match AppModal's scroll lane so header and bar stay aligned even when volumes overflow. */
.dl-card:not(.dl-card-error) .n-card-header__main { overflow: hidden; scrollbar-gutter: stable; padding-right: 2px; }
.dl-card .n-card-content { padding: 0 var(--dl-gutter) 16px; }
.dl-card .app-modal-scroll { gap: 8px; }
.dl-card .app-modal-footer { margin-top: 4px; padding-top: 8px; border-top: 0; }
</style>

<style scoped>
.dl-heading { display: flex; align-items: center; gap: 12px; }
.dl-heading-icon {
  display: grid; place-items: center; flex: 0 0 42px; aspect-ratio: 1; border-radius: 50%;
  color: var(--oc2-accent); background: color-mix(in srgb, currentColor 14%, transparent);
}
.dl-card-error .dl-heading-icon { color: var(--oc2-tone-critical-text); }
.dl-heading-text { flex: 1; min-width: 0; }
.dl-title, .dl-file { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dl-title { margin: 0; font-size: 17px; line-height: 1.35; font-weight: 650; color: var(--oc2-text-1); }
.dl-file { display: block; margin: 2px 0 0; font-size: 13px; line-height: 1.35; font-weight: 500; color: var(--oc2-text-2); }
.dl-heading :deep(.dl-cancel:focus-visible), .dl-actions :deep(button:focus-visible) { outline: 2px solid var(--oc2-accent); outline-offset: -2px; }
.dl-body { display: grid; gap: 8px; min-width: 0; }
.dl-progress-row, .dl-bar { min-width: 0; }
.dl-percent {
  flex: none; min-inline-size: 5.6ch; text-align: end; font-size: 28px; line-height: 1.1;
  font-weight: 700; font-variant-numeric: tabular-nums; color: var(--oc2-text-1); white-space: nowrap;
}
.dl-percent > span { margin-inline-start: 2px; font-size: .65em; }
.dl-stats { display: grid; gap: 8px; font-size: 13px; color: var(--oc2-text-2); font-variant-numeric: tabular-nums; }
.dl-amount-rate, .dl-state-route { display: flex; align-items: center; gap: 12px; min-width: 0; }
.dl-amount, .dl-rate { display: inline-flex; align-items: center; gap: 6px; }
.dl-amount svg, .dl-rate svg { flex: none; color: var(--oc2-text-3); }
.dl-rate { flex: none; margin-inline-start: auto; white-space: nowrap; }
.dl-rate { font-weight: 650; }
.dl-state-route { border-top: 1px solid var(--oc2-row-line); padding-top: 8px; font-size: 12px; }
.dl-route { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.dl-route::before {
  content: ''; display: inline-block; width: 6px; height: 6px; margin-inline-end: 8px; vertical-align: .15em;
  border-radius: 50%; background: var(--oc2-text-3);
}
.dl-state-route.is-transferring .dl-route::before { background: var(--oc2-accent); }
.dl-error { overflow-wrap: anywhere; }
.dl-error-label { margin: 0; font-size: 13px; font-weight: 600; color: var(--oc2-text-2); }
.dl-error-reason { margin: 4px 0 0; font-size: 13px; line-height: 1.45; color: var(--oc2-text-1); white-space: pre-wrap; }
.dl-actions { display: flex; justify-content: flex-end; gap: 8px; flex-wrap: wrap; }

@media (prefers-reduced-motion: reduce) {
  .dl-bar :deep(.n-progress-graph-line-fill),
  .dl-bar :deep(.n-progress-graph-line-fill::after) { animation: none; transition: none; }
}
</style>
