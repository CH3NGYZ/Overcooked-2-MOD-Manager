<script setup lang="ts">
import { computed, shallowRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import { appUpdateState, canCheckAppUpdate, canExitForAppUpdate, presentAppUpdate, type UpdateExitAction } from '../appUpdateState'
import { checkForAppUpdate, openAppUpdatePage, useAppUpdatePromptOpen } from '../composables/useAppUpdates'
import { downloadAmount, downloadPercent, downloadRate } from '../downloadPresentation'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { useNotify } from '../useNotify'
import AppUpdateNotes from './AppUpdateNotes.vue'
import TitleBarProgressTable from './TitleBarProgressTable.vue'
import TitleBarStatusPopover from './TitleBarStatusPopover.vue'

const notify = useNotify()
const emit = defineEmits<{ 'update-exit': [action: UpdateExitAction] }>()
const appUpdatePromptOpen = useAppUpdatePromptOpen()
const open = shallowRef(false)
const stagedForExit = computed(() => appUpdateState.stage === 'ready' && appUpdateState.deferred)
watch(appUpdatePromptOpen, visible => { if (visible) open.value = false })
const storeManaged = computed(() => appUpdateState.distribution === 'microsoft_store')
const checking = computed(() => appUpdateState.stage === 'checking' || appUpdateState.recheckingStatus)
const downloading = computed(() => appUpdateState.stage === 'downloading')
const checkFailed = computed(() => appUpdateState.stage === 'checkFailed')
const tone = computed(() => appUpdateState.error || checkFailed.value || appUpdateState.stage === 'restartFailed' ? 'error'
  : appUpdatePromptOpen.value || appUpdateState.stage === 'ready' ? 'warning'
    : checking.value || downloading.value ? 'activity' : undefined)
const status = computed(() => {
  if (storeManaged.value) return t('update.microsoftStoreOnly')
  if (checking.value) return t('update.checking')
  switch (appUpdateState.stage) {
    case 'current': return t('update.alreadyLatest')
    case 'checkFailed': return t('dialogs.updateCheckFailed')
    case 'downloading': return t(appUpdateState.progress?.finished || appUpdateState.progress?.phase === 'verifying'
      ? 'update.verifyingUpdate' : 'update.downloadingInstaller')
    case 'ready': return t(appUpdateState.deferred ? 'update.installPending' : 'update.actionRequired')
    case 'systemReboot': return t('update.rebootRequired')
    case 'restartFailed': return t('update.restartFailed')
    case 'applying': case 'systemApplying': return t('update.applyingUpdate')
    default: return t(appUpdatePromptOpen.value ? 'update.actionRequired' : 'update.notChecked')
  }
})
const rows = computed(() => {
  const rows = [{ label: t('update.status'), value: status.value }]
  const progress = appUpdateState.progress
  if (downloading.value && progress) {
    rows.push({ label: t('update.progress'), value: downloadAmount(progress) })
    const rate = downloadRate(progress)
    if (rate) rows.push({ label: t('update.speed'), value: rate })
  }
  if (appUpdateState.resolvingExitBlock && appUpdateState.error) {
    rows.push({ label: t('update.exitBlocked'), value: backendMessageOf(appUpdateState.error) })
  }
  return rows
})
const percent = computed(() => downloading.value ? (appUpdateState.progress ? downloadPercent(appUpdateState.progress) : null) : undefined)
const canCheck = computed(canCheckAppUpdate)
const showCheck = computed(() => (canCheck.value || checking.value || storeManaged.value) && !appUpdatePromptOpen.value)

function applyUpdate() {
  if (!canExitForAppUpdate('apply')) return
  open.value = false
  emit('update-exit', 'apply')
}

async function check() {
  if (!canCheck.value && !storeManaged.value) return
  try { await checkForAppUpdate() }
  catch (reason) { notify.error(backendMessageOf(reason)) }
}

async function openReleasePage() {
  try { await openAppUpdatePage() }
  catch (reason) { notify.error(backendMessageOf(reason)) }
}
</script>

<template>
  <TitleBarStatusPopover v-model:open="open" :label="t('update.managerUpdate')" :title="status" :heading="t('update.managerUpdate')"
    :tone="tone" :rotating="downloading" data-testid="app-update-status" :data-update-stage="appUpdateState.stage">
    <template #icon><RefreshOutline width="17" height="17" aria-hidden="true" /></template>
    <template #heading-action>
      <NButton v-if="showCheck" size="small" secondary :loading="checking" :aria-disabled="checking"
        data-testid="app-update-check" @click="check">
        {{ storeManaged ? t('overview.check') : t('common.action.refetch') }}
      </NButton>
    </template>
    <TitleBarProgressTable :label="t('update.managerUpdate')" :rows="rows" :progress="percent" />
    <template v-if="stagedForExit">
      <AppUpdateNotes class="update-notes-preview" :update="appUpdateState.update" tabindex="0" data-testid="app-update-notes-preview" />
      <div class="update-actions">
        <NButton size="small" type="primary" :disabled="!canExitForAppUpdate('apply')" data-testid="app-update-preview-apply"
          @click="applyUpdate">{{ t('update.applyNow') }}</NButton>
      </div>
    </template>
    <NButton v-else-if="appUpdateState.resolvingExitBlock" size="small" secondary data-testid="app-update-show" @click="presentAppUpdate">
      {{ t('update.managerUpdate') }}
    </NButton>
    <div v-if="checkFailed && showCheck" class="update-actions">
      <NButton size="small" quaternary @click="openReleasePage">{{ t('dialogs.openReleasePage') }}</NButton>
    </div>
  </TitleBarStatusPopover>
</template>

<style scoped>
.update-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
.update-notes-preview { max-height: min(180px, 40vh); overflow: auto; overflow-wrap: anywhere; scrollbar-gutter: stable; }
</style>
