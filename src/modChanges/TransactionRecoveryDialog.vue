<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, ref, shallowRef, useTemplateRef, watch } from 'vue'
import { NAlert, NButton, NTag } from 'naive-ui'
import AppModal from '../components/AppModal.vue'
import { useDashboard } from '../composables/useDashboardStore'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { isTauri } from '../runtime'
import { api, watchModChangeRecovery } from '../services/tauri'
import ChangeEvidenceList from './ChangeEvidenceList.vue'
import { showTransactionRecovery, transactionRecoveries as records, transactionRecoveryLoading as loading,
  transactionRecoveryReadError as readError, transactionRecoveryVisible as visible } from './transactionRecovery'
import type { TransactionRecoveryEntry } from './transactionRecoveryTypes'

const dashboard = useDashboard()
const working = ref<string | null>(null)
const operationError = shallowRef<unknown>(null)
const confirming = shallowRef<{ record: TransactionRecoveryEntry; highRisk: boolean } | null>(null)
const confirmationHeading = useTemplateRef<HTMLElement>('confirmationHeading')
const gameRunning = computed(() => !!dashboard.game.value?.kitchen_running)
const canWrite = computed(() => showTransactionRecovery.value && !loading.value && !readError.value && !working.value && !gameRunning.value)
const groups = computed(() => [
  { key: 'pending', entries: records.value.filter(record => !record.archived) },
  { key: 'history', entries: records.value.filter(record => record.archived) },
])
const pathKinds = ['target', 'backup', 'staging', 'parent', 'journal'] as const
const recoveryKey = (key: string) => `modChanges.transactionRecovery.${key}`

let disposed = false
let revision = 0
let unlisten: (() => void) | undefined
async function refresh() {
  const mine = ++revision
  loading.value = true
  try {
    const entries = await api.modChangeRecoveries()
    if (!disposed && mine === revision) {
      records.value = entries
      readError.value = null
    }
  } catch (error) {
    if (!disposed && mine === revision) readError.value = error
  } finally {
    if (!disposed && mine === revision) loading.value = false
  }
}

function currentRecord(record: TransactionRecoveryEntry) {
  return records.value.find(item => item.id === record.id && item.revision === record.revision
    && item.can_decide && !item.archived)
}

async function decide(record: TransactionRecoveryEntry, action: 'pause' | 'retry' | 'abandon', highRisk = false) {
  const current = currentRecord(record)
  if (!canWrite.value || !current || (action === 'abandon' && !current.scope_known && !highRisk)) return
  if (action !== 'abandon' && current.state === 'abandoned') return
  working.value = record.id
  operationError.value = null
  const mine = ++revision
  try {
    const entries = await (action === 'pause' ? api.pauseModChangeRecovery(record.id, record.revision)
      : action === 'retry' ? api.retryModChangeRecovery(record.id, record.revision)
        : api.abandonModChangeRecovery(record.id, record.revision, highRisk))
    if (!disposed && mine === revision) records.value = entries
  } catch (error) {
    operationError.value = error
    if (!disposed) await refresh()
  } finally {
    working.value = null
  }
}

function requestAbandon(record: TransactionRecoveryEntry) {
  if (canWrite.value && currentRecord(record)) confirming.value = { record, highRisk: false }
}

function confirmAbandon(event: MouseEvent) {
  const decision = confirming.value
  if (event.detail > 1 || !decision || !canWrite.value || !currentRecord(decision.record)) return
  if (!decision.record.scope_known && !decision.highRisk) {
    confirming.value = { record: decision.record, highRisk: true }
    return
  }
  confirming.value = null
  void decide(decision.record, 'abandon', decision.highRisk)
}

watch(confirming, async decision => {
  if (!decision) return
  await nextTick()
  if (confirming.value === decision) confirmationHeading.value?.focus({ preventScroll: true })
}, { flush: 'post' })
watch(records, () => {
  if (confirming.value && !currentRecord(confirming.value.record)) confirming.value = null
})
watch(() => dashboard.game.value?.path, () => { confirming.value = null }, { flush: 'sync' })
watch(showTransactionRecovery, shown => {
  if (!shown) { confirming.value = null; visible.value = false }
  else { operationError.value = null; void refresh() }
})
onMounted(async () => {
  if (!isTauri()) return
  try {
    const stop = await watchModChangeRecovery(() => { if (!disposed) void refresh() })
    if (disposed) { stop(); return }
    unlisten = stop
  } catch {} finally {
    if (!disposed) void refresh()
  }
})
onBeforeUnmount(() => { disposed = true; unlisten?.() })
</script>

<template>
  <AppModal :show="showTransactionRecovery" :title="t(recoveryKey('title'))" :z-index="2601"
    max-width="min(740px, 96%)" :mask-closable="false" :closable="!working && !confirming"
    @update:show="(show: boolean) => { if (!show && !working && !confirming) visible = false }">
    <section v-if="confirming" :key="String(confirming.highRisk)" class="transaction-recovery" data-transaction-recovery-confirmation
      :data-high-risk="confirming.highRisk">
      <h3 ref="confirmationHeading" tabindex="-1">{{ t(recoveryKey(confirming.highRisk ? 'highRiskTitle' : 'abandon')) }}</h3>
      <NAlert type="warning" :bordered="false">
        {{ t(recoveryKey(confirming.highRisk ? 'highRiskDescription' : 'abandonDescription')) }}
      </NAlert>
      <code>{{ confirming.record.id }}</code>
    </section>
    <section v-else class="transaction-recovery" data-transaction-recovery-panel>
      <p>{{ t(recoveryKey('description')) }}</p>
      <p v-if="loading" role="status">{{ t('common.loading') }}</p>
      <NAlert v-if="readError || operationError" type="error" :bordered="false" data-transaction-recovery-error>
        {{ backendMessageOf(operationError || readError) }}
      </NAlert>
      <NAlert v-if="gameRunning && groups[0].entries.length" type="warning" :bordered="false">{{ backendMessageOf('game.running') }}</NAlert>
      <p v-if="!loading && !readError && !records.length">{{ t(recoveryKey('empty')) }}</p>
      <template v-for="group in groups" :key="group.key">
        <section v-if="group.entries.length" :data-transaction-recovery-group="group.key">
          <h3>{{ t(recoveryKey(group.key)) }}</h3>
          <article v-for="record in group.entries" :key="record.id" class="recovery-record"
            :data-transaction-recovery-record="record.id" :data-transaction-recovery-state="record.state">
            <div class="record-heading">
              <code>{{ record.id }}</code>
              <NTag size="small" :type="!record.archived && record.state !== 'paused' ? 'warning' : 'default'">
                {{ record.state === 'abandoned' && !record.archived ? t('modChanges.recoveryChanged') : t(recoveryKey(`state.${record.state}`)) }}
              </NTag>
            </div>
            <dl>
              <dt>{{ t(recoveryKey('applicationError')) }}</dt>
              <dd>{{ record.application_error ? backendMessageOf(record.application_error) : t(recoveryKey('notRecorded')) }}</dd>
              <dt>{{ t(recoveryKey('recoveryError')) }}</dt>
              <dd>{{ record.recovery_error ? backendMessageOf(record.recovery_error) : t(recoveryKey('notRecorded')) }}</dd>
              <template v-if="record.legacy_error">
                <dt>{{ t(recoveryKey('legacyError')) }}</dt>
                <dd>{{ backendMessageOf(record.legacy_error) }}</dd>
              </template>
            </dl>
            <p v-if="!record.scope_known">{{ t(recoveryKey('unknownPaths')) }}</p>
            <template v-for="kind in pathKinds" :key="kind">
              <div v-if="record.paths.some(path => path.kind === kind)" class="recovery-paths">
                <strong>{{ t(recoveryKey(`path.${kind}`)) }}</strong>
                <ChangeEvidenceList :rows="record.paths.filter(path => path.kind === kind)" />
              </div>
            </template>
            <div v-if="record.can_decide && !record.archived" class="recovery-buttons">
              <NButton v-if="record.state !== 'abandoned'" size="small" :disabled="!canWrite"
                data-transaction-recovery-action="retry" @click="decide(record, 'retry')">
                {{ t(recoveryKey('retry')) }}
              </NButton>
              <NButton v-if="record.state === 'automatic'" size="small" :disabled="!canWrite"
                data-transaction-recovery-action="pause" @click="decide(record, 'pause')">
                {{ t(recoveryKey('pause')) }}
              </NButton>
              <NButton size="small" :disabled="!canWrite" type="warning" secondary
                data-transaction-recovery-action="abandon" @click="requestAbandon(record)">
                {{ t(recoveryKey('abandon')) }}
              </NButton>
            </div>
          </article>
        </section>
      </template>
    </section>
    <template #footer>
      <div class="recovery-buttons">
        <template v-if="confirming">
          <NButton data-transaction-recovery-action="cancel-abandon" @click="confirming = null">{{ t('common.action.cancel') }}</NButton>
          <NButton :key="String(confirming.highRisk)" type="warning" :disabled="!canWrite"
            data-transaction-recovery-action="confirm-abandon" @click="confirmAbandon"
            @keydown.enter="(event: KeyboardEvent) => { if (event.repeat) event.preventDefault() }">
            {{ t(recoveryKey(confirming.highRisk ? 'highRiskConfirm' : confirming.record.scope_known ? 'abandon' : 'continue')) }}
          </NButton>
        </template>
        <template v-else>
          <NButton v-if="readError" :disabled="loading || !!working" @click="refresh">{{ t(recoveryKey('refresh')) }}</NButton>
          <NButton :disabled="!!working" data-transaction-recovery-action="close" @click="visible = false">{{ t('nav.close') }}</NButton>
        </template>
      </div>
    </template>
  </AppModal>
</template>

<style scoped>
.transaction-recovery { min-width: 0; overflow-wrap: anywhere; }
.transaction-recovery p { margin: 0 0 8px; }
.transaction-recovery h3 { margin: 8px 0; font-size: 1em; }
.recovery-record { border: 1px solid var(--oc2-row-line); border-radius: 8px; padding: 10px; margin-block-end: 8px; }
.record-heading { display: flex; flex-wrap: wrap; align-items: baseline; gap: 8px; font-weight: 600; }
.record-heading code { min-width: 0; }
.recovery-record dl { margin: 10px 0; }
.recovery-record dt { font-weight: 600; }
.recovery-record dd { margin: 0 0 4px; white-space: pre-wrap; }
.recovery-paths { margin-block: 8px; }
.recovery-buttons { display: flex; justify-content: flex-end; flex-wrap: wrap; gap: 6px; width: 100%; min-width: 0; }
.recovery-buttons :deep(.n-button) { max-width: 100%; height: auto; min-height: var(--n-height); }
.recovery-buttons :deep(.n-button__content) { white-space: normal; }
</style>
