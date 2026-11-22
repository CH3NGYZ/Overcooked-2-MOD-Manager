<script setup lang="ts">
import { computed } from 'vue'
import { NButton } from 'naive-ui'
import {
  appUpdateState, dismissAppUpdateNotes, type UpdateRecoveryDestination,
} from '../appUpdateState'
import { useAppUpdatePromptMode } from '../composables/useAppUpdates'
import { t } from '../i18n'
import { translateBackendMessage } from '../locale/backend'
import AppModal from './AppModal.vue'
import MarkdownBody from './MarkdownBody.vue'

const emit = defineEmits<{ 'apply-update': []; 'recover-update': [destination: UpdateRecoveryDestination] }>()
defineProps<{ confirmingClose: boolean }>()
const appUpdatePromptMode = useAppUpdatePromptMode()
const recovery = computed(() => appUpdatePromptMode.value === 'recovery')
const openingRecovery = computed(() => appUpdateState.stage === 'openingRecovery')
const applying = computed(() => ['applying', 'handoff'].includes(appUpdateState.stage))
const update = computed(() => appUpdateState.update)
const notes = computed(() => update.value?.body?.trim() ?? '')
</script>

<template>
  <AppModal :show="recovery || (!confirmingClose && appUpdatePromptMode === 'notes')" exclude-from-dialog-queue
    :closable="!recovery && !applying" :mask-closable="false"
    :z-index="6100" max-width="min(520px, 92%)" max-height="85%" min-height="100px"
    :aria-label="recovery ? t('update.applicationFailedTitle') : t('update.releaseNotes')"
    :data-testid="recovery ? 'app-update-recovery' : 'app-update-notes'"
    @update:show="(show: boolean) => { if (!show) dismissAppUpdateNotes() }">
    <div v-if="recovery" class="update-body">
      <h2>{{ t('update.applicationFailedTitle') }}</h2>
      <p>{{ t('update.applicationFailed') }}</p>
      <p v-if="appUpdateState.error" class="update-error" role="alert">{{ translateBackendMessage(appUpdateState.error) }}</p>
    </div>
    <div v-else class="update-body">
      <div class="update-heading">
        <h2>{{ t('dialogs.versionAvailable', { version: 'v' + update?.version }) }}</h2>
        <span v-if="update" class="version-line muted">v{{ update.current_version }} → v{{ update.version }}</span>
      </div>
      <MarkdownBody v-if="notes" class="update-notes" :source="notes" />
      <div v-else class="update-notes">{{ t('dialogs.noReleaseNotesWereProvided') }}</div>
      <p v-if="applying" role="status">{{ t('update.applyingUpdate') }}</p>
    </div>
    <template #footer>
      <div v-if="recovery" class="update-footer">
        <NButton secondary :disabled="openingRecovery" data-testid="app-update-alternate" @click="emit('recover-update', 'alternate')">
          {{ t('update.goToAlternateDownload') }}
        </NButton>
        <NButton type="primary" :disabled="openingRecovery" data-testid="app-update-website" @click="emit('recover-update', 'website')">
          {{ t('update.goToWebsite') }}
        </NButton>
      </div>
      <div v-else class="update-footer">
        <NButton secondary :disabled="applying" data-testid="app-update-install-later" @click="dismissAppUpdateNotes">
          {{ t('update.installLater') }}
        </NButton>
        <NButton type="primary" :loading="applying" :disabled="applying" data-testid="app-update-install-now" @click="emit('apply-update')">
          {{ t('update.applyNow') }}
        </NButton>
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
.update-notes {
  padding: 10px 12px; border: 1px solid var(--oc2-divider); border-radius: 8px;
  background: var(--oc2-sunken); font-size: 13px; line-height: 1.6; color: var(--oc2-text-2);
}
.update-error { color: var(--oc2-status-error); overflow-wrap: anywhere; }
.update-footer { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
</style>
