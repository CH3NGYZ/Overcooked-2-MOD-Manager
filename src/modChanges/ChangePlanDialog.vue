<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { NAlert, NButton, NSpin, NTag } from 'naive-ui'
import AppModal from '../components/AppModal.vue'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { useModChanges } from './store'
import { useModChangesLifecycle } from './useModChangesLifecycle'
import { useDashboard } from '../composables/useDashboardStore'
import { changePlanInstallNotes } from './planNotes'
import { renderReadme } from '../markdown'
import { changeReasonKeys } from './messages'
function reasonText(reason: string) { return changeReasonKeys[reason] ? t(changeReasonKeys[reason]!) : translateBackendMessage(reason) }

const changes = useModChanges()
useModChangesLifecycle()
const dashboard = useDashboard()
const state = changes.state
const plan = computed(() => state.value?.plan)
const waiting = computed(() => ['planning', 'preparing', 'executing'].includes(state.value?.phase ?? ''))
const canCancel = computed(() => !['preparing', 'executing'].includes(state.value?.phase ?? ''))
const labels = computed(() => new Map(plan.value?.nodes.map(node => [node.node_id, node.label]) ?? []))
const changedNodes = computed(() => plan.value?.nodes.filter(node => node.members?.length
  && plan.value?.actions.some(action => action.node_id === node.node_id)) ?? [])
function memberLabel(nodeId: string, memberKey: string) {
  return plan.value?.nodes.find(node => node.node_id === nodeId)?.members?.find(member => member.key === memberKey)?.dll_file ?? memberKey
}
const error = computed(() => state.value?.error ? backendMessageOf(state.value.error) : '')
const phase = computed(() => t(`modChanges.phase.${state.value?.phase ?? 'planning'}`))
const notes = computed(() => changePlanInstallNotes(plan.value, dashboard.config.value?.repositories ?? [], dashboard.mods.value))
const readingSeconds = ref(0)
let readingTimer: ReturnType<typeof setInterval> | undefined
function stopReadingTimer() { if (readingTimer !== undefined) clearInterval(readingTimer); readingTimer = undefined }
watch(() => notes.value.map(note => `${note.id}:${note.text}`).join('\n'), () => {
  stopReadingTimer()
  const until = Date.now() + notes.value.length * 5000
  readingSeconds.value = notes.value.length * 5
  if (readingSeconds.value) readingTimer = setInterval(() => {
    readingSeconds.value = Math.max(0, Math.ceil((until - Date.now()) / 1000))
    if (!readingSeconds.value) stopReadingTimer()
  }, 100)
}, { immediate: true })
onUnmounted(stopReadingTimer)
const actionKeys = {
  install: 'modChanges.action.install', upgrade: 'modChanges.action.upgrade',
  downgrade: 'modChanges.action.downgrade', replace: 'modChanges.action.replace',
  enable: 'modChanges.action.enable', disable: 'modChanges.action.disable',
} as const
</script>

<template>
  <AppModal :show="!!state" :title="t('modChanges.title')" max-width="min(760px, 94%)"
    :closable="canCancel" :mask-closable="false" card-class="mod-changes-modal"
    @update:show="(show: boolean) => { if (!show) void changes.cancel() }">
    <div class="change-plan" data-mod-changes-plan>
      <p class="plan-phase" role="status"><NSpin v-if="waiting" size="small" />{{ phase }}</p>
      <NAlert v-if="state?.phase === 'reconfirming'" type="warning" :bordered="false">
        {{ t('modChanges.changed') }}
      </NAlert>
      <section v-if="plan?.roots.length" class="plan-roots">
        <div v-for="root in plan.roots" :key="root.node_id" class="plan-root">
          <strong>{{ root.label }}</strong><NTag v-if="root.tag" size="small">{{ root.tag }}</NTag>
        </div>
      </section>
      <NAlert v-if="plan && plan.status !== 'ready'" type="error" :bordered="false">
        {{ t(plan.status === 'incomplete' ? 'modChanges.incomplete' : 'modChanges.unsatisfiable') }}
      </NAlert>
      <NAlert v-if="error" type="error" :bordered="false">{{ error }}</NAlert>
      <section v-if="notes.length" class="plan-install-notes">
        <article v-for="note in notes" :key="note.id"><strong>{{ note.name }}</strong>
          <div class="install-note-markdown" v-html="renderReadme(note.text)"></div>
        </article>
        <p v-if="readingSeconds" role="status">{{ t('modChanges.readingNotes', { seconds: readingSeconds }) }}</p>
      </section>
      <ul v-if="plan?.blockers.length" class="plan-notes">
        <li v-for="(blocker, index) in plan.blockers" :key="index">
          {{ reasonText(blocker.message || blocker.code) }}
          <span v-if="blocker.owner_id"> · {{ labels.get(blocker.owner_id) ?? blocker.owner_id }}</span>
          <span v-if="blocker.target_guid"> · {{ blocker.target_guid }}</span>
        </li>
      </ul>
      <div v-if="plan?.actions.length" class="plan-actions">
        <article v-for="(action, index) in plan.actions" :key="`${action.node_id}:${action.kind}:${index}`" class="plan-action">
          <NTag :type="action.kind === 'disable' ? 'warning' : 'info'" size="small">{{ t(actionKeys[action.kind]) }}</NTag>
          <div class="action-content">
            <strong>{{ labels.get(action.node_id) ?? action.node_id }}</strong>
            <span v-if="action.member_key" class="action-member"> · {{ memberLabel(action.node_id, action.member_key) }}</span>
            <div v-if="action.from_tag || action.to_tag" class="action-version">{{ action.from_tag ?? '—' }} → {{ action.to_tag ?? '—' }}</div>
            <p v-if="action.reason" class="action-reason">{{ reasonText(action.reason) }}</p>
            <p v-if="action.caused_by.length" class="action-reason">{{ t('modChanges.causedBy', { names: action.caused_by.map(id => labels.get(id) ?? id).join(', ') }) }}</p>
          </div>
        </article>
      </div>
      <p v-else-if="plan?.status === 'ready'" class="plan-empty">{{ t('modChanges.noChanges') }}</p>
      <section v-if="changedNodes.length" class="plan-package-members">
        <h4>{{ t('modChanges.members') }}</h4>
        <article v-for="node in changedNodes" :key="node.node_id">
          <strong>{{ node.label }}</strong>
          <ul>
            <li v-for="member in node.members" :key="member.key">
              <NTag :type="member.enabled ? 'success' : 'default'" size="small">{{ t(member.enabled ? 'modChanges.action.enable' : 'modChanges.action.disable') }}</NTag>
              <span>{{ member.dll_file }}</span>
            </li>
          </ul>
        </article>
      </section>
      <ul v-if="plan?.impacts.length" class="plan-notes">
        <li v-for="(impact, index) in plan.impacts" :key="index">{{ translateBackendMessage(impact) }}</li>
      </ul>
      <p v-if="plan?.status === 'ready'" class="plan-footer-note">{{ t('modChanges.atomicNotice') }}</p>
    </div>
    <template #footer>
      <NButton :disabled="!canCancel" @click="changes.cancel">{{ t('common.action.cancel') }}</NButton>
      <NButton type="primary" :loading="waiting" :disabled="waiting || state?.phase === 'failed' || readingSeconds > 0 || plan?.status !== 'ready'"
        @click="changes.confirm">{{ t(state?.phase === 'reconfirming' ? 'modChanges.reconfirm' : 'modChanges.confirm') }}</NButton>
    </template>
  </AppModal>
</template>

<style scoped>
.change-plan { display: flex; flex-direction: column; gap: 14px; min-width: 0; }
.plan-phase, .plan-root { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; margin: 0; }
.plan-roots { display: flex; flex-direction: column; gap: 8px; }
.plan-root strong { min-width: 0; overflow-wrap: anywhere; }
.plan-actions { display: flex; flex-direction: column; gap: 10px; }
.plan-action { display: flex; align-items: flex-start; gap: 12px; padding: 12px; border: 1px solid var(--oc2-row-line); border-radius: 10px; }
.action-content { min-width: 0; overflow-wrap: anywhere; }
.action-version { margin-top: 4px; font-variant-numeric: tabular-nums; }
.action-reason, .plan-footer-note { margin: 5px 0 0; opacity: .75; font-size: 12px; line-height: 1.6; }
.plan-notes { margin: 0; padding-left: 20px; overflow-wrap: anywhere; }
.plan-empty { margin: 0; }
.plan-package-members h4 { margin: 0 0 10px; }
.plan-package-members ul { display: flex; flex-direction: column; gap: 8px; margin: 8px 0; padding-left: 18px; }
.plan-package-members li { overflow-wrap: anywhere; }
.plan-package-members li > span { margin-left: 8px; }
.plan-install-notes { padding: 12px; border-radius: 10px; background: var(--oc2-sunken); }
.install-note-markdown { overflow-wrap: anywhere; line-height: 1.6; }
.install-note-markdown :deep(img), .install-note-markdown :deep(svg) { max-width: 100%; }
.install-note-markdown :deep(pre) { overflow-x: auto; }
</style>
