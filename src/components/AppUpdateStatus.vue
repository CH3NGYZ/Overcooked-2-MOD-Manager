<script setup lang="ts">
import { computed, shallowRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import { appUpdateState, canCheckAppUpdate, canRetryAppUpdateDownload } from '../appUpdateState'
import { checkForAppUpdate, retryAppUpdateDownload } from '../composables/useAppUpdates'
import { downloadAmount, downloadPercent, downloadRate } from '../downloadPresentation'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import TitleBarProgressTable from './TitleBarProgressTable.vue'
import TitleBarStatusPopover from './TitleBarStatusPopover.vue'

const emit = defineEmits<{ 'apply-update': [] }>()
const props = defineProps<{ promptOpen: boolean }>()
const notify = useNotify()
const open = shallowRef(false)
watch(() => props.promptOpen, promptOpen => { if (promptOpen) open.value = false })
const storeManaged = computed(() => appUpdateState.distribution === 'microsoft_store')
const checking = computed(() => appUpdateState.stage === 'checking')
const downloading = computed(() => appUpdateState.stage === 'downloading')
const ready = computed(() => appUpdateState.stage === 'ready')
const downloadFailed = computed(() => appUpdateState.stage === 'stageFailed')
const checkFailed = computed(() => appUpdateState.stage === 'checkFailed')
const recovery = computed(() => ['recovery', 'openingRecovery'].includes(appUpdateState.stage))
const update = computed(() => appUpdateState.update)
const progress = computed(() => appUpdateState.progress)
const percent = computed(() => downloading.value && progress.value ? downloadPercent(progress.value) : null)
const progressBar = computed(() => ready.value ? 100 : downloading.value ? percent.value : undefined)
const tone = computed(() => ready.value ? 'warning'
  : downloadFailed.value || checkFailed.value || recovery.value || appUpdateState.stage === 'restartFailed' ? 'error'
    : checking.value || downloading.value ? 'activity' : undefined)
const status = computed(() => {
  if (storeManaged.value) return t('update.microsoftStoreOnly')
  switch (appUpdateState.stage) {
    case 'checking': return t('update.checking')
    case 'current': return t('update.alreadyLatest')
    case 'checkFailed': return t('dialogs.updateCheckFailed')
    case 'downloading': return t(progress.value?.finished || progress.value?.phase === 'finished'
      ? 'update.verifyingUpdate' : 'update.downloadingInstaller')
    case 'stageFailed': return t(appUpdateState.error === 'update.installFailedManual'
      ? 'update.applicationFailedTitle' : 'update.downloadFailed')
    case 'ready': return t('update.pendingInstall')
    case 'systemReboot': return t('update.rebootRequired')
    case 'restartFailed': return t('update.restartFailed')
    case 'applying': case 'handoff': case 'systemApplying': return t('update.applyingUpdate')
    case 'recovery': case 'openingRecovery': return t('update.applicationFailedTitle')
    default: return t('update.notChecked')
  }
})
const rows = computed(() => {
  const rows = [{ label: t('update.status'), value: status.value }]
  if (update.value) rows.push({ label: t('update.targetVersion'), value: `v${update.value.version}` })
  if (downloading.value && progress.value) {
    rows.push({ label: t('update.progress'), value: downloadAmount(progress.value) })
    const rate = downloadRate(progress.value)
    if (rate) rows.push({ label: t('update.speed'), value: rate })
  }
  return rows
})
const canCheck = computed(canCheckAppUpdate)
const canRetry = computed(canRetryAppUpdateDownload)
const openRelease = computed(() => checkFailed.value || downloadFailed.value && !canRetry.value)

async function check() {
  try { await checkForAppUpdate() }
  catch (reason) { notify.error(backendMessageOf(reason)) }
}

async function openReleasePage() {
  try {
    const result = await api.openUpdateRepository()
    if (!result.ok) notify.error(result.message)
  } catch (reason) { notify.error(backendMessageOf(reason)) }
}

function apply() {
  open.value = false
  emit('apply-update')
}
</script>

<template>
  <TitleBarStatusPopover v-model:open="open" :label="t('update.managerUpdate')" :title="status"
    :tone="tone" :rotating="downloading" data-testid="app-update-status" :data-update-stage="appUpdateState.stage">
    <template #icon><RefreshOutline width="17" height="17" aria-hidden="true" /></template>
    <strong>{{ t('update.managerUpdate') }}</strong>
    <TitleBarProgressTable :label="t('update.managerUpdate')" :rows="rows" :progress="progressBar" />
    <p v-if="downloadFailed && appUpdateState.error" class="update-detail" role="alert">{{ translateBackendMessage(appUpdateState.error) }}</p>
    <div v-if="ready || canCheck" class="update-actions">
      <NButton v-if="ready" size="small" type="primary" secondary data-testid="app-update-install" @click="apply">
        {{ t('update.applyNow') }}
      </NButton>
      <template v-else>
        <NButton v-if="openRelease" size="small" quaternary @click="openReleasePage">{{ t('dialogs.openReleasePage') }}</NButton>
        <NButton size="small" secondary data-testid="app-update-check" @click="check">
          {{ storeManaged ? t('overview.check') : t('update.checkAgain') }}
        </NButton>
        <NButton v-if="canRetry" size="small" type="primary" data-testid="app-update-retry" @click="retryAppUpdateDownload">
          {{ t('update.retryDownload') }}
        </NButton>
      </template>
    </div>
  </TitleBarStatusPopover>
</template>

<style scoped>
.update-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
.update-detail { margin: 0; color: var(--oc2-status-error); font-size: 12px; overflow-wrap: anywhere; max-height: 90px; overflow-y: auto; }
</style>
