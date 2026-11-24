<script setup lang="ts">
import { onMounted, shallowRef } from 'vue'
import { NAlert, NButton, NTooltip } from 'naive-ui'
import { RefreshOutline, WarningOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import type { LevelCompressionRecovery } from '../types'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { useNotify } from '../useNotify'
import { useDialog } from 'naive-ui'
import { markDialogBusy } from '../dialogBusy'
import { useModChanges } from './store'

const notify = useNotify()
const dialog = useDialog()
const changes = useModChanges()
const recoveries = shallowRef<LevelCompressionRecovery[]>([])
const loading = shallowRef(false)
const working = shallowRef<string | null>(null)

async function load() {
  loading.value = true
  try { recoveries.value = await api.levelCompressionRecoveries() }
  catch (reason) { notify.error(backendMessageOf(reason)) }
  finally { loading.value = false }
}

async function retry(item: LevelCompressionRecovery) {
  if (working.value) return
  working.value = item.id
  try {
    recoveries.value = await api.retryLevelCompressionRecovery(item.id)
    if (!recoveries.value.some(recovery => recovery.id === item.id)) {
      notify.success(t('modChanges.compressionRecovery.recovered'))
      await changes.retry()
    }
  } catch (reason) { notify.error(backendMessageOf(reason)) }
  finally { working.value = null }
}

function ignore(item: LevelCompressionRecovery) {
  if (working.value) return
  const instance = dialog.warning({
    zIndex: 2601,
    title: t('modChanges.compressionRecovery.title'),
    content: t('modChanges.compressionRecovery.ignoreConfirm'),
    positiveText: t('modChanges.compressionRecovery.confirmIgnore'),
    negativeText: t('modChanges.compressionRecovery.cancelIgnore'),
    onPositiveClick: async () => {
      if (working.value) return false
      markDialogBusy(instance)
      working.value = item.id
      try {
        recoveries.value = await api.ignoreLevelCompressionRecovery(item.id)
        if (!recoveries.value.some(recovery => recovery.id === item.id)) {
          notify.warning(t('modChanges.compressionRecovery.ignored'))
          await changes.retry()
        }
      } catch (reason) { notify.error(backendMessageOf(reason)); return false }
      finally { working.value = null }
    },
  })
}

onMounted(load)
</script>

<template>
  <NAlert type="warning" :bordered="false" data-compression-recovery>
    <div class="recovery-heading">{{ t('modChanges.compressionRecovery.title') }}</div>
    <p class="recovery-description">{{ t('modChanges.compressionRecovery.description') }}</p>
    <p v-if="loading" class="recovery-empty" role="status">{{ t('common.loading') }}</p>
    <p v-else-if="!recoveries.length" class="recovery-empty">{{ t('modChanges.compressionRecovery.unavailable') }}</p>
    <ul v-else class="recovery-list">
      <li v-for="item in recoveries" :key="item.id" class="recovery-item">
        <div class="recovery-paths">
          <div><strong>{{ t('modChanges.compressionRecovery.source') }}</strong><code :title="item.source_path">{{ item.source_path }}</code></div>
          <div><strong>{{ t('modChanges.compressionRecovery.backup') }}</strong><code :title="item.backup_path">{{ item.backup_path }}</code></div>
          <div v-if="item.restore_error"><strong>{{ t('modChanges.compressionRecovery.error') }}</strong><code>{{ item.restore_error }}</code></div>
        </div>
        <div class="recovery-actions">
          <NTooltip placement="top" :delay="150">
            <template #trigger>
              <NButton quaternary circle size="small" :loading="working === item.id" :disabled="!!working || loading"
                :aria-label="t('modChanges.compressionRecovery.retry')" @click="retry(item)">
                <template #icon><RefreshOutline /></template>
              </NButton>
            </template>
            {{ t('modChanges.compressionRecovery.retry') }}
          </NTooltip>
          <NTooltip placement="top" :delay="150">
            <template #trigger>
              <NButton quaternary circle size="small" type="warning" :disabled="!!working || loading"
                :aria-label="t('modChanges.compressionRecovery.ignore')" @click="ignore(item)">
                <template #icon><WarningOutline /></template>
              </NButton>
            </template>
            {{ t('modChanges.compressionRecovery.ignore') }}
          </NTooltip>
        </div>
      </li>
    </ul>
  </NAlert>
</template>

<style scoped>
.recovery-heading { font-weight: 600; }
.recovery-description { margin: 4px 0 8px; white-space: pre-wrap; overflow-wrap: anywhere; }
.recovery-empty { margin: 4px 0 0; font-size: 12px; }
.recovery-list { list-style: none; margin: 0; padding: 0; display: flex; flex-direction: column; gap: 8px; }
.recovery-item { display: flex; align-items: flex-start; gap: 12px; min-width: 0; padding-top: 8px; border-top: 1px solid var(--oc2-row-line); }
.recovery-paths { display: flex; flex: 1 1 auto; flex-direction: column; gap: 4px; min-width: 0; }
.recovery-paths > div { display: grid; grid-template-columns: minmax(88px, max-content) minmax(0, 1fr); gap: 8px; min-width: 0; }
.recovery-paths strong { font-size: 12px; color: var(--oc2-text-2); }
.recovery-paths code { min-width: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.recovery-actions { display: flex; flex: 0 0 auto; gap: 4px; }
@media (max-width: 640px) { .recovery-item { flex-direction: column; } .recovery-actions { align-self: flex-end; } }
</style>
