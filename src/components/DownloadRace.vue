<script setup lang="ts">
import { computed, onBeforeUpdate, onScopeDispose, onUpdated, shallowRef, useId, useTemplateRef, watch } from 'vue'
import { NTooltip } from 'naive-ui'
import { ChevronDownOutline, ChevronUpOutline, LockClosedOutline, LockOpenOutline, RefreshOutline } from '@vicons/ionicons5'
import type { DownloadRaceProgress, DownloadRaceSource } from '../types'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { api } from '../services/tauri'
import { createDownloadRaceRanking, isActiveDownloadSource, visibleDownloadSources } from '../downloadRaceRanking'
import { createDownloadRaceRetirement } from '../downloadRaceRetirement'
import DownloadRaceRow from './DownloadRaceRow.vue'
import HeaderIconAction from './HeaderIconAction.vue'

const props = defineProps<{ race: DownloadRaceProgress; interactive?: boolean }>()
type Action = 'lock' | 'retry' | 'unlock'

const rank = createDownloadRaceRanking()
const trackRetirement = createDownloadRaceRetirement()
const ranked = shallowRef<DownloadRaceSource[]>([])
const retirement = shallowRef(trackRetirement(props.race, performance.now()))
const expanded = shallowRef(false)
const retiredExpanded = shallowRef(false)
const layoutChanged = shallowRef(false)
const pending = shallowRef<Action | null>(null)
const error = shallowRef('')
const focusedSource = shallowRef<number | null>(null)
const panel = useTemplateRef<HTMLElement>('panel')
const listId = useId()
const retiredListId = useId()
const headingId = useId()
let revision = 0
let restore: HTMLElement | null = null
let retirementTimer: ReturnType<typeof setTimeout> | undefined

watch(() => props.race.id, () => {
  revision += 1
  expanded.value = false
  retiredExpanded.value = false
  focusedSource.value = null
  pending.value = null
  error.value = ''
}, { flush: 'sync' })

function updateRetirement() {
  clearTimeout(retirementTimer)
  const now = performance.now()
  const next = trackRetirement(props.race, now)
  retirement.value = next
  if (!next.folded.size) retiredExpanded.value = false
  if (Number.isFinite(next.nextExpiry)) retirementTimer = setTimeout(updateRetirement, Math.ceil(next.nextExpiry - now))
}

watch(() => props.race, race => {
  ranked.value = rank(race)
  updateRetirement()
}, { immediate: true, flush: 'sync' })
onScopeDispose(() => clearTimeout(retirementTimer))

const locked = computed(() => props.race.locked_id != null)
const active = computed(() => props.race.sources.filter(isActiveDownloadSource).length)
const liveRows = computed(() => ranked.value.filter(source => !retirement.value.folded.has(source.id)))
const retiredRows = computed(() => ranked.value.filter(source => retirement.value.folded.has(source.id)))
const defaultRows = computed(() => visibleDownloadSources(liveRows.value, false, props.race.primary_id, props.race.locked_id, focusedSource.value))
const rows = computed(() => expanded.value ? liveRows.value : defaultRows.value)
const retiredLabel = computed(() => t(retiredExpanded.value ? 'tools.downloadRace.collapseRetired' : 'tools.downloadRace.expandRetired'))
watch([rows, retiredRows, retiredExpanded], ([next, nextRetired, nextOpen], [previous, previousRetired, previousOpen]) => {
  layoutChanged.value = next.length !== previous.length
    || next.some(source => !previous.some(row => row.id === source.id))
    || nextOpen !== previousOpen
    || (nextOpen ? nextRetired.length !== previousRetired.length : !!nextRetired.length !== !!previousRetired.length)
})

const canLock = (source: DownloadRaceSource) => props.interactive && props.race.can_lock && !locked.value
  && (source.phase === 'connecting' || source.phase === 'downloading')
const isLocked = (source: DownloadRaceSource) => locked.value && props.race.locked_id === source.id
const canRetry = (source: DownloadRaceSource) => props.interactive && isLocked(source) && source.phase === 'failed'

async function control(action: Action, source: DownloadRaceSource) {
  if (!props.interactive || pending.value) return
  if (action === 'lock' ? !canLock(source) : action === 'retry' ? !canRetry(source) : !isLocked(source)) return
  const request = ++revision
  pending.value = action
  error.value = ''
  try {
    await api.controlDownloadRace(props.race.id, action, action === 'lock' ? source.id : undefined)
  } catch (reason) {
    if (revision === request) error.value = backendMessageOf(reason)
  } finally {
    if (revision === request) pending.value = null
  }
}

function trackFocus(event: FocusEvent) {
  const row = (event.target as HTMLElement).closest<HTMLElement>('[data-race-source]')
  focusedSource.value = row ? Number(row.dataset.raceSource) : null
}

onBeforeUpdate(() => {
  const activeElement = document.activeElement
  restore = activeElement instanceof HTMLElement && panel.value?.contains(activeElement) ? activeElement : null
})
onUpdated(() => {
  if (restore && document.activeElement === document.body) {
    (restore.isConnected ? restore : panel.value?.querySelector<HTMLElement>('.dl-race-retired-toggle') ?? panel.value)?.focus({ preventScroll: true })
  }
  restore = null
})
</script>

<template>
  <section ref="panel" class="dl-race-panel" :aria-labelledby="headingId" tabindex="-1" @focusin="trackFocus"
    @focusout="($event.relatedTarget && !panel?.contains($event.relatedTarget as Node)) && (focusedSource = null)">
    <div class="dl-race-heading">
      <h3 :id="headingId">{{ t('tools.downloadRace.heading') }}</h3>
      <span v-if="active">{{ t('tools.downloadRace.active', { count: active }) }}</span>
      <span v-if="race.waiting">{{ t(locked ? 'tools.downloadRace.paused' : 'tools.downloadRace.waiting', { count: race.waiting }) }}</span>
    </div>

    <TransitionGroup :id="listId" name="race-row" tag="ul" class="dl-race-list" :class="{ 'is-layout-change': layoutChanged }"
      @leave="(_row, remove) => remove()">
      <DownloadRaceRow v-for="source in rows" :key="`${race.id}:${source.id}`" :source="source"
        :primary="race.primary_id === source.id" :locked="isLocked(source)" :show-reason="isLocked(source)">
        <HeaderIconAction v-if="canRetry(source)" class="dl-race-retry" size="tiny" quaternary circle hover-only
          :label="t('tools.downloadRace.retry')" :aria-disabled="!!pending" :loading="pending === 'retry'" @click="control('retry', source)">
          <RefreshOutline aria-hidden="true" />
        </HeaderIconAction>
        <HeaderIconAction v-if="interactive && (canLock(source) || isLocked(source) || focusedSource === source.id)" class="dl-race-lock"
          size="tiny" quaternary circle hover-only :aria-pressed="isLocked(source)" :aria-disabled="!!pending || (!canLock(source) && !isLocked(source))"
          :label="t(isLocked(source) ? 'tools.downloadRace.unlock' : 'tools.downloadRace.lock')" @click="control(isLocked(source) ? 'unlock' : 'lock', source)">
          <LockClosedOutline v-if="isLocked(source)" aria-hidden="true" /><LockOpenOutline v-else aria-hidden="true" />
        </HeaderIconAction>
      </DownloadRaceRow>
    </TransitionGroup>

    <p v-if="error" class="dl-race-error" role="alert">{{ error }}</p>
    <HeaderIconAction v-if="liveRows.length > defaultRows.length" size="tiny" quaternary circle hover-only class="dl-race-more"
      :label="expanded ? t('tools.downloadRace.collapse') : t('tools.downloadRace.expand', { count: liveRows.length })"
      :aria-expanded="expanded" :aria-controls="listId" @click="expanded = !expanded">
      <ChevronUpOutline v-if="expanded" aria-hidden="true" /><ChevronDownOutline v-else aria-hidden="true" />
    </HeaderIconAction>
    <div v-if="retiredRows.length" class="dl-race-retired-summary">
      <NTooltip placement="top" :delay="150" :keep-alive-on-hover="false">
        <template #trigger>
          <button type="button" tabindex="0" class="dl-race-retired-toggle" :aria-expanded="retiredExpanded" :aria-controls="retiredListId"
            :aria-description="retiredLabel" @click="retiredExpanded = !retiredExpanded">
            <span v-if="retirement.counts.failed" class="dl-race-retired-failed">{{ t('tools.downloadRace.failedCount', { count: retirement.counts.failed }) }}</span>
            <span v-if="retirement.counts.stopped">{{ t('tools.downloadRace.stoppedCount', { count: retirement.counts.stopped }) }}</span>
            <ChevronUpOutline v-if="retiredExpanded" aria-hidden="true" /><ChevronDownOutline v-else aria-hidden="true" />
          </button>
        </template>
        {{ retiredLabel }}
      </NTooltip>
      <ul :id="retiredListId" class="dl-race-list dl-race-retired-list" :hidden="!retiredExpanded">
        <DownloadRaceRow v-for="source in retiredExpanded ? retiredRows : []" :key="source.id" :source="source" show-reason />
      </ul>
    </div>
  </section>
</template>

<style scoped>
.dl-race-panel { min-width: 0; border-top: 1px solid var(--oc2-row-line); padding-top: 6px; font-size: 12px; color: var(--oc2-text-2); }
.dl-race-heading { display: flex; align-items: center; gap: 6px; min-width: 0; flex-wrap: wrap; padding: 2px 0 4px; }
.dl-race-heading h3 { margin: 0 auto 0 0; margin-inline: 0 auto; font-size: 12px; color: var(--oc2-text-1); }
.dl-race-heading > span { color: var(--oc2-text-3); font-variant-numeric: tabular-nums; }
.dl-race-list { position: relative; list-style: none; margin: 0; padding: 0; }
.dl-race-panel :deep(button[aria-disabled='true']) { opacity: .5; }
.dl-race-panel :deep(button:focus-visible), .dl-race-panel:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: -1px; }
.dl-race-error { color: var(--oc2-status-error); margin: 4px 0 0; overflow-wrap: anywhere; }
.dl-race-panel :deep(.dl-race-more) { margin-top: 4px; }
.dl-race-retired-summary { border-top: 1px solid var(--oc2-row-line); padding-top: 3px; }
.dl-race-retired-toggle {
  display: flex; align-items: center; gap: 10px; width: 100%; min-height: 24px; padding: 2px 4px; border: 0;
  border-radius: 4px; background: transparent; color: var(--oc2-text-3); font: inherit; text-align: start; cursor: pointer;
}
.dl-race-retired-toggle:hover { background: var(--oc2-sunken); color: var(--oc2-text-2); }
.dl-race-retired-toggle svg { margin-inline-start: auto; width: 14px; height: 14px; flex: none; }
.dl-race-retired-failed { color: var(--oc2-status-error); }
.dl-race-list:not(.is-layout-change) .race-row-move { transition: transform 250ms ease-out; }
.race-row-enter-active { transition: opacity 150ms ease-out; }
.race-row-enter-from { opacity: 0; }
@media (prefers-reduced-motion: reduce) {
  .dl-race-list:not(.is-layout-change) .race-row-move, .race-row-enter-active { transition: none; }
}
</style>
