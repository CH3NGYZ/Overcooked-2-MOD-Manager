<script setup lang="ts">
import { computed, h, nextTick, onBeforeUnmount, onMounted, shallowRef, watch } from 'vue'
import { NButton, NCheckbox, NDropdown, type DropdownOption } from 'naive-ui'
import { EllipsisHorizontal, FolderOpenOutline, RefreshOutline, SaveOutline, ShirtOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { canBeDefault, hatForChef, isUsableResource } from '../diychef/draft'
import type { HatReference } from '../diychef/types'
import { projectChefs, projectProblems, type ChefFilters as ChefFilterState } from '../diychef/projection'
import { useDiyChefEditor } from '../composables/useDiyChefEditor'
import { useDiyChefConfirmations } from '../composables/useDiyChefConfirmations'
import { usePageLeaveGuard } from '../composables/usePageLeaveGuard'
import { useChefReorder } from '../composables/useChefReorder'
import { useChefMotion } from '../composables/useChefMotion'
import { useChefSelection } from '../composables/useChefSelection'
import { useDashboard } from '../composables/useDashboardStore'
import { configurePreviewConcurrency } from '../diychef/preview/queue'
import { diyChefDemoSnapshot } from '../tutorial/demoDiyChef'
import HeaderSearch from '../components/HeaderSearch.vue'
import HeaderIconAction from '../components/HeaderIconAction.vue'
import ContentState from '../components/ContentState.vue'
import ModNotInstalled from '../components/ModNotInstalled.vue'
import ChefGrid from '../components/diychef/ChefGrid.vue'
import ChefFilters from '../components/diychef/ChefFilters.vue'
import EditorNotice from '../components/diychef/EditorNotice.vue'
import ConfigProblems from '../components/diychef/ConfigProblems.vue'
import HatDialog from '../components/diychef/HatDialog.vue'
import TargetPicker from '../components/diychef/TargetPicker.vue'
import SaveChangesDialog from '../components/diychef/SaveChangesDialog.vue'
import LeaveDialog from '../components/diychef/LeaveDialog.vue'
import LegacyMigrationNotice from '../components/diychef/LegacyMigrationNotice.vue'

const message = useNotify()
const dashboard = useDashboard()
watch(() => dashboard.config.value?.diychef_preview_concurrency ?? 0, configurePreviewConcurrency, { immediate: true })
const confirmation = useDiyChefConfirmations(() => editor.save())
const editor = useDiyChefEditor({ confirmChanges: confirmation.confirmChanges, cancelChanges: confirmation.cancelChanges, demoSnapshot: () => diyChefDemoSnapshot.value, confirmMigration: () => requestLeave(),
  interactionBusy: () => !!draggingId.value || !!selection.rectangle.value || hatOpen.value || moreOpen.value || leaveOpen.value || changesOpen.value })
const { snapshot, state, opened, loading, saving, error, diagnostics, gameRunning, readOnly, stale, busy, editable, dirty, canSave, canUndo, demonstrating } = editor
const { changes, changesOpen, leaveOpen } = confirmation
const { requestLeave } = usePageLeaveGuard({ busy, dirty, confirm: confirmation.confirmLeave, cancelConfirm: confirmation.cancelLeave })
const query = shallowRef('')
const sources = shallowRef<ChefFilterState['sources']>([])
const statuses = shallowRef<ChefFilterState['statuses']>([])
const filters = computed<ChefFilterState>(() => ({ query: query.value, sources: sources.value, statuses: statuses.value }))
const projection = computed(() => snapshot.value && state.value ? projectChefs(snapshot.value, state.value, filters.value) : null)
const filterKey = computed(() => JSON.stringify(filters.value))
const readingOrder = computed(() => projection.value ? [...projection.value.enabled, ...projection.value.disabled].map(item => item.chef.id) : [])
const selectableIds = computed(() => new Set(snapshot.value?.chefs.filter(chef => isUsableResource(chef.resource_state)).map(chef => chef.id) ?? []))
const sessionKey = computed(() => snapshot.value?.session_id)
const allProjection = computed(() => snapshot.value && state.value ? projectChefs(snapshot.value, state.value, { query: '', sources: [], statuses: [] }) : null)
const canEnableAll = computed(() => !!allProjection.value?.disabled.some(item => isUsableResource(item.chef.resource_state)))
const problems = computed(() => snapshot.value && state.value ? projectProblems(snapshot.value, state.value, diagnostics.value) : [])
const hatChefId = shallowRef<string | null>(null)
const hatScope = shallowRef<'chef' | 'all'>('chef')
const hatOpen = shallowRef(false)
const moreOpen = shallowRef(false)
const hatChef = computed(() => snapshot.value?.chefs.find((chef) => chef.id === hatChefId.value) ?? null)
const currentHat = computed(() => snapshot.value && state.value && hatChefId.value ? hatForChef(state.value, snapshot.value, hatChefId.value) : null)
const hatChefEnabled = computed(() => state.value?.draft.entries.some((entry) => entry.chef_ref.kind === 'known' && entry.chef_ref.chef_id === hatChefId.value) ?? false)
const announcement = shallowRef('')
const root = shallowRef<HTMLElement | null>(null)
const chefMotion = useChefMotion(root)
const reorderable = computed(() => editable.value && !projection.value?.filtered && !hatOpen.value && !leaveOpen.value && !changesOpen.value)
const selectionEnabled = computed(() => editable.value && !hatOpen.value && !leaveOpen.value && !changesOpen.value)
const selection = useChefSelection({ root, enabled: selectionEnabled, filterKey, sessionId: sessionKey, readingOrder, selectableIds })
const { draggingId, draggingIds, overEnabled, start: startDrag } = useChefReorder({
  root, enabled: reorderable, members: selection.members, place, disable, motion: chefMotion,
  canBeDefault: source => !!snapshot.value && canBeDefault(source, snapshot.value),
})
const moreOptions = computed<DropdownOption[]>(() => [
  { key: 'undo', label: t('diychef.undo'), disabled: !canUndo.value },
  { type: 'divider', key: 'divider' },
  { key: 'default', label: t('diychef.restoreDefaults'), disabled: !editable.value },
  { type: 'divider', key: 'lobby-divider' },
  { key: 'lobby', disabled: !editable.value, label: () => h(NCheckbox, {
    checked: state.value?.draft.lobby_switch ?? false, disabled: !editable.value,
    tabindex: -1, 'aria-hidden': true, style: { pointerEvents: 'none' },
  }, { default: () => t('diychef.lobbySwitch') }) },
])
const moreNodeProps = (option: DropdownOption) => option.key === 'lobby'
  ? { role: 'menuitemcheckbox', 'aria-label': t('diychef.lobbySwitch'), 'aria-checked': state.value?.draft.lobby_switch ? 'true' as const : 'false' as const, 'data-tutorial': 'diychef-more-lobby' }
  : option.key === 'default' ? { 'data-tutorial': 'diychef-more-default' } : {}

function clearFilters() { query.value = ''; sources.value = []; statuses.value = [] }
async function focusChef(chefId: string, scroll = false) {
  await nextTick()
  const button = document.querySelector<HTMLButtonElement>(`[data-chef-focus="${CSS.escape(chefId)}"]`)
  const tile = button?.closest<HTMLElement>('[data-chef-id]')
  if (button && !button.disabled) button.focus({ preventScroll: true })
  else tile?.querySelector<HTMLInputElement>('.chef-select input:not(:disabled)')?.focus({ preventScroll: true })
  if (!tile) document.querySelector<HTMLButtonElement>('.diychef-page .chef-tile-main:not(:disabled)')?.focus({ preventScroll: true })
  if (scroll) tile?.scrollIntoView({ block: 'nearest', behavior: 'auto' })
}
function announcePosition(chefId: string) {
  const entries = state.value?.draft.entries.filter((entry) => entry.chef_ref.kind === 'known') ?? []
  const index = entries.findIndex((entry) => entry.chef_ref.kind === 'known' && entry.chef_ref.chef_id === chefId)
  const chef = snapshot.value?.chefs.find((entry) => entry.id === chefId)
  if (index >= 0 && chef) announcement.value = t('diychef.moved', { name: chef.display_name, position: index + 1 })
}
function place(chefIds: string | readonly string[], targetChefId: string | null, after: boolean) {
  if (!reorderable.value) return
  const anchor = draggingId.value ?? (typeof chefIds === 'string' ? chefIds : chefIds[0]!)
  void chefMotion.change(chefIds, () => {
    editor.place(chefIds, targetChefId, after)
    announcePosition(anchor)
  }, () => focusChef(anchor, true))
}
function disable(chefIds: string | readonly string[]) {
  const ids = typeof chefIds === 'string' ? [chefIds] : chefIds
  const anchor = draggingId.value ?? ids[0]!
  void chefMotion.change(ids, () => editor.disable(ids), () => focusChef(anchor))
}
function selectChef(chefId: string) { selection.toggle(chefId) }
function bulkEnable() { selection.clear(); void chefMotion.change(allProjection.value?.disabled.map(item => item.chef.id) ?? [], editor.enableAll) }
function bulkDisable() { selection.clear(); void chefMotion.change(allProjection.value?.enabled.map(item => item.chef.id) ?? [], editor.disableAll) }
function openHat(chefId: string, scope: 'chef' | 'all' = 'chef') {
  hatScope.value = scope
  hatChefId.value = chefId
  hatOpen.value = true
}
function applyHat(chefId: string, hat: HatReference) {
  editor.changeHat(hatScope.value === 'all' ? snapshot.value?.chefs.map(chef => chef.id) ?? [] : chefId, hat)
}
function more(key: string) {
  if (key === 'undo') editor.undo()
  else if (key === 'default') editor.reset('restore_default')
  else if (key === 'lobby') editor.setLobby(!state.value?.draft.lobby_switch)
}
async function openFolder() {
  if (demonstrating.value || !snapshot.value) return
  try {
    const result = await api.openModFileLocation(snapshot.value.target.display_path)
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) { message.error(backendMessageOf(reason)) }
}
async function reload() {
  if (demonstrating.value) return
  if (await requestLeave()) { hatOpen.value = false; await editor.open(undefined, true) }
}
function saveShortcut(event: KeyboardEvent) {
  if (!(event.ctrlKey || event.metaKey) || event.key.toLowerCase() !== 's') return
  event.preventDefault()
  if (!hatOpen.value && !leaveOpen.value && !changesOpen.value) void editor.save()
}
watch(demonstrating, () => {
  hatOpen.value = false
  moreOpen.value = false
  confirmation.cancelLeave()
  clearFilters()
}, { flush: 'sync' })
onMounted(() => { void editor.open(); window.addEventListener('keydown', saveShortcut) })
onBeforeUnmount(() => window.removeEventListener('keydown', saveShortcut))
</script>

<template>
  <Teleport to="#page-header-title-extra" defer>
    <ChefFilters v-if="projection" v-model:sources="sources" v-model:statuses="statuses" :counts="projection.filterCounts" />
  </Teleport>
  <Teleport to="#page-header-actions" defer>
    <HeaderSearch v-model:value="query" />
    <HeaderIconAction :label="t('diychef.bulkChangeHat')" :disabled="!editable || !projection?.defaultId" data-tutorial="diychef-bulk-hat"
      @click="projection?.defaultId && openHat(projection.defaultId, 'all')"><ShirtOutline /></HeaderIconAction>
    <HeaderIconAction :label="t('diychef.reload')" :disabled="busy || demonstrating" @click="reload"><RefreshOutline /></HeaderIconAction>
    <HeaderIconAction :label="t('modsLocal.openFolder')" :disabled="!snapshot || busy || demonstrating" @click="openFolder"><FolderOpenOutline /></HeaderIconAction>
    <n-dropdown v-model:show="moreOpen" :options="moreOptions" :node-props="moreNodeProps" trigger="click" placement="bottom-end" @select="more">
      <span class="chef-more-trigger" data-tutorial="diychef-more"><HeaderIconAction :label="t('diychef.more')" aria-haspopup="menu" :aria-expanded="moreOpen"><EllipsisHorizontal /></HeaderIconAction></span>
    </n-dropdown>
    <n-button size="small" type="primary" :loading="saving" :disabled="!canSave" @click="editor.save">
      <template #icon><SaveOutline /></template>{{ t('diychef.save') }}
    </n-button>
  </Teleport>
  <div ref="root" class="diychef-page" data-chef-motion="idle" data-tutorial="diychef-page" @pointerdown.capture="selection.start">
    <LegacyMigrationNotice v-if="editor.migrationVisible.value" :targets="editor.legacyTargets.value" :busy="busy" :importing="editor.migrating.value" :can-import="editor.canMigrate.value" :target-available="opened?.migration_available === true" @import="editor.migrate" @ignore="editor.ignoreMigration" />
    <EditorNotice :snapshot="snapshot" :error="error" :game-running="gameRunning" :read-only="readOnly" :stale="stale" :busy="busy" :diagnostics="diagnostics" @reload="reload" />
    <ContentState v-if="loading && !snapshot" loading :description="t('diychef.loading')" />
    <TargetPicker v-else-if="opened?.status === 'select_target'" :candidates="opened.candidates" :busy="busy" @select="editor.open" />
    <ModNotInstalled v-else-if="opened?.status === 'not_installed'" guid="dev.gua.overcooked.diychef" name="DIYChef" />
    <template v-if="snapshot && state && projection">
      <ConfigProblems :problems="problems" :frozen="!editable" @remove="editor.removeProblem" @hat="openHat" />
      <ChefGrid data-tutorial="diychef-grid" :items="projection.enabled" :session-id="snapshot.session_id" enabled :filtered="projection.filtered" :frozen="!editable"
        :bulk-disabled="!projection.defaultId"
        :reorderable="reorderable" :dragging-id="draggingId" :dragging-ids="draggingIds" :selected-ids="selection.selectedIds.value" :over-enabled="overEnabled"
        @select="selectChef" @hat="openHat" @drag="startDrag" @bulk-disable="bulkDisable" @bulk-enable="bulkEnable" />
      <ChefGrid :items="projection.disabled" :session-id="snapshot.session_id" :enabled="false" :filtered="projection.filtered" :frozen="!editable"
        :bulk-disabled="!canEnableAll"
        :reorderable="reorderable" :dragging-id="draggingId" :dragging-ids="draggingIds" :selected-ids="selection.selectedIds.value" :over-enabled="overEnabled"
        @select="selectChef" @hat="openHat" @drag="startDrag" @bulk-disable="bulkDisable" @bulk-enable="bulkEnable" />
      <div v-if="selection.rectangle.value" class="chef-selection-rect" :style="selection.rectangleStyle.value" aria-hidden="true" />
      <ContentState v-if="!projection.enabled.length && !projection.disabled.length" :description="t('diychef.noResults')">
        <template #extra><n-button size="small" @click="clearFilters">{{ t('diychef.clearFilters') }}</n-button></template>
      </ContentState>
      <HatDialog v-model:show="hatOpen" :snapshot="snapshot" :chef="hatChef" :hat="currentHat" :enabled="hatChefEnabled" :scope="hatScope" :frozen="!editable" @apply="applyHat" />
    </template>
    <SaveChangesDialog :show="changesOpen" :changes="changes" @resolve="confirmation.resolveChanges" />
    <LeaveDialog :show="leaveOpen" :can-save="canSave" @stay="confirmation.cancelLeave" @discard="confirmation.resolveLeave(true)" @save="confirmation.saveAndLeave" />
    <span class="chef-live" aria-live="polite" aria-atomic="true">{{ announcement }}</span>
  </div>
</template>

<style scoped>
.diychef-page { position: relative; display: flex; flex-direction: column; flex: 1 1 auto; min-width: 0; min-height: 0; }
.chef-more-trigger { display: inline-flex; }
.chef-live { position: absolute; width: 1px; height: 1px; padding: 0; overflow: hidden; clip: rect(0, 0, 0, 0); white-space: nowrap; border: 0; }
.chef-selection-rect { position: fixed; z-index: 25; pointer-events: none; border: 1px solid var(--oc2-accent); background: color-mix(in srgb, var(--oc2-accent) 12%, transparent); }
</style>
