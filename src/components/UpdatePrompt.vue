<script setup lang="ts">
import { computed } from 'vue'
import { NButton } from 'naive-ui'
import { appUpdateState, canCheckAppUpdate, canExitForAppUpdate, type UpdateExitAction } from '../appUpdateState'
import { checkForAppUpdate, deferAppUpdate, useAppUpdatePromptOpen } from '../composables/useAppUpdates'
import { t } from '../i18n'
import { translateBackendMessage } from '../locale/backend'
import AppModal from './AppModal.vue'
import AppUpdateNotes from './AppUpdateNotes.vue'

const emit = defineEmits<{ 'update-exit': [action: UpdateExitAction] }>()
defineProps<{ confirmingClose: boolean }>()
const appUpdatePromptOpen = useAppUpdatePromptOpen()
const update = computed(() => appUpdateState.update)
const stage = computed(() => appUpdateState.stage)
const asUser = computed(() => update.value?.install_mode === 'restartAsUser' && ['ready', 'applying'].includes(stage.value))
const error = computed(() => appUpdateState.error)
const normalUpdate = computed(() => update.value?.install_mode === 'deferred' && ['ready', 'deferring', 'applying'].includes(stage.value))
const showDownload = computed(() => canExitForAppUpdate('download') || ['openingRelease', 'openingWebsite'].includes(stage.value)
  || (stage.value === 'applying' && !normalUpdate.value))
const showLater = computed(() => normalUpdate.value && stage.value !== 'applying')
const showApply = computed(() => canExitForAppUpdate('apply') || stage.value === 'applying')
const showReboot = computed(() => ['systemReboot', 'exitingForReboot'].includes(stage.value))
const showStatusCheck = computed(() => ['systemApplying', 'systemReboot'].includes(stage.value))
const status = computed(() => {
  switch (stage.value) {
    case 'systemReboot': case 'exitingForReboot': return t('update.rebootRequired')
    case 'systemApplying': return t('update.waitForInstaller')
    case 'openingRelease': return t('update.openingReleasePage')
    case 'openingWebsite': return t('update.openingWebsite')
    case 'applying': return t(asUser.value ? 'update.reopeningAsUser' : 'update.applyingUpdate')
    default: return ''
  }
})
</script>

<template>
  <AppModal :show="appUpdatePromptOpen && !confirmingClose" :closable="false" :mask-closable="false" exclude-from-dialog-queue
    :z-index="6100" max-width="min(520px, 92%)" max-height="85%" min-height="100px"
    :aria-label="t('update.managerUpdate')" data-testid="app-update-notes">
    <div class="update-body">
      <div class="update-heading">
        <h2>{{ t('dialogs.versionAvailable', { version: 'v' + update?.version }) }}</h2>
        <span v-if="update" class="version-line muted">v{{ update.current_version }} → v{{ update.version }}</span>
      </div>
      <p v-if="asUser && !showStatusCheck" data-testid="app-update-permission-info">{{ t('update.restartAsUserHelp') }}</p>
      <p v-else-if="update?.install_mode === 'downloadAndExit' && !showStatusCheck"
        data-testid="app-update-permission-info">
        {{ t('update.restartAsUserUnavailable') }}
      </p>
      <p v-if="showDownload">{{ t('update.downloadAndExitHelp') }}</p>
      <p v-if="normalUpdate">{{ t('update.installOnExitHelp') }}</p>
      <p v-if="error" class="update-error" role="alert">{{ translateBackendMessage(error) }}</p>
      <p v-if="status" role="status">{{ status }}</p>
      <AppUpdateNotes :update="update" />
    </div>
    <template #footer>
      <div class="update-footer">
        <NButton v-if="showStatusCheck" secondary :loading="appUpdateState.recheckingStatus" :disabled="!canCheckAppUpdate()"
          data-testid="app-update-check-again" @click="checkForAppUpdate()">{{ t('update.checkAgain') }}</NButton>
        <NButton v-if="showReboot" type="primary" :loading="stage === 'exitingForReboot'" :disabled="!canExitForAppUpdate('reboot')"
          data-testid="app-update-quit-to-reboot" @click="emit('update-exit', 'reboot')">{{ t('update.quitToRestart') }}</NButton>
        <NButton v-if="showDownload" secondary :loading="stage === 'openingWebsite'" :disabled="!canExitForAppUpdate('website')"
          data-testid="app-update-website" @click="emit('update-exit', 'website')">{{ t('update.goToWebsite') }}</NButton>
        <NButton v-if="showDownload" :type="showApply ? 'default' : 'primary'"
          :secondary="showApply" :loading="stage === 'openingRelease'" :disabled="!canExitForAppUpdate('download')"
          data-testid="app-update-manual" @click="emit('update-exit', 'download')">{{ t('dialogs.openReleasePage') }}</NButton>
        <NButton v-if="showLater" secondary :loading="stage === 'deferring'"
          :disabled="stage !== 'ready'" data-testid="app-update-install-later" @click="deferAppUpdate">{{ t('update.installLater') }}</NButton>
        <NButton v-if="showApply || stage === 'deferring'" type="primary" :loading="stage === 'applying'" :disabled="!canExitForAppUpdate('apply')"
          data-testid="app-update-install-now" @click="emit('update-exit', 'apply')">{{ t(asUser ? 'update.restartAsUserAction' : 'update.applyNow') }}</NButton>
      </div>
    </template>
  </AppModal>
</template>

<style scoped>
.update-body { display: flex; flex-direction: column; gap: 10px; }
.update-body h2 { margin: 0; font-size: 21px; line-height: 1.3; }
.update-body p { margin: 0; }
.update-heading { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 12px; }
.version-line { font-size: 13px; white-space: nowrap; }
.update-error { color: var(--oc2-status-error); overflow-wrap: anywhere; }
.update-footer { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
.update-footer :deep(.n-button) { min-width: 0; max-width: 100%; height: auto; min-height: var(--n-height); white-space: normal; }
.update-footer :deep(.n-button__content) { min-width: 0; overflow-wrap: anywhere; }
</style>
