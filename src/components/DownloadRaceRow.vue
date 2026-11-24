<script setup lang="ts">
import { computed } from 'vue'
import type { DownloadRaceSource } from '../types'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { downloadAmount, downloadPercent, downloadRate, downloadRoute } from '../downloadPresentation'

const props = defineProps<{
  source: DownloadRaceSource
  primary?: boolean
  locked?: boolean
  showReason?: boolean
}>()

const name = computed(() => downloadRoute(props.source))
const percent = computed(() => downloadPercent(props.source))
const status = computed(() => props.source.phase === 'downloading' ? downloadRate(props.source) : t(`tools.downloadRace.${props.source.phase}`))
const reason = computed(() => props.source.error ? backendMessageOf(props.source.error) : '')
</script>

<template>
  <li class="dl-race-row" :class="{ 'is-primary': primary, 'is-failed': source.phase === 'failed' }"
    :data-race-source="source.id" :data-race-phase="source.phase">
    <div class="dl-race-top">
      <div class="dl-race-labels">
        <span class="dl-race-name" :title="name">{{ name }}</span>
        <span v-if="locked" class="dl-race-badge is-primary">{{ t('tools.downloadRace.locked') }}</span>
        <span v-else-if="primary" class="dl-race-badge is-primary">{{ t('tools.downloadRace.preferred') }}</span>
        <span v-if="source.history_preferred" class="dl-race-badge">{{ t('tools.downloadRace.historyPreferred') }}</span>
      </div>
      <slot />
    </div>
    <div class="dl-race-meta">
      <div class="dl-race-track" role="progressbar" :aria-label="name" :aria-valuenow="percent ?? undefined"
        :aria-valuetext="downloadAmount(source)">
        <span :style="{ transform: `scaleX(${(percent ?? 0) / 100})` }" />
      </div>
      <span class="dl-race-size">{{ downloadAmount(source) }}</span>
      <span class="dl-race-status" :title="showReason ? undefined : reason || undefined">{{ status }}</span>
    </div>
    <p v-if="reason && showReason" class="dl-race-error" :role="locked ? 'alert' : undefined">{{ reason }}</p>
  </li>
</template>

<style scoped>
.dl-race-row { padding: 3px 0; min-width: 0; border-top: 1px solid var(--oc2-row-line); }
.dl-race-top, .dl-race-labels { display: flex; align-items: center; gap: 6px; min-width: 0; }
.dl-race-top { min-height: 23px; }
.dl-race-labels { flex: 1; }
.dl-race-name { min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; color: var(--oc2-text-1); font-weight: 600; }
.dl-race-badge { flex: none; padding-inline: 5px; border-radius: 4px; background: var(--oc2-sunken); color: var(--oc2-text-2); font-size: 11px; white-space: nowrap; }
.dl-race-badge.is-primary { color: var(--oc2-accent); background: color-mix(in srgb, var(--oc2-accent) 13%, transparent); }
.dl-race-meta { display: flex; align-items: center; flex-wrap: wrap; gap: 2px 8px; font-variant-numeric: tabular-nums; line-height: 1.25; }
.dl-race-track { flex: 1 1 32px; min-width: 16px; height: 3px; border-radius: 3px; background: var(--oc2-row-line); overflow: hidden; }
.dl-race-track span { display: block; height: 100%; background: var(--oc2-text-3); transform-origin: left center; }
.is-primary .dl-race-track span { background: var(--oc2-accent); }
.dl-race-status { min-width: 8ch; text-align: end; }
.is-failed .dl-race-status, .dl-race-error { color: var(--oc2-status-error); }
.dl-race-error { margin: 4px 0 0; overflow-wrap: anywhere; }
.dl-race-size { color: var(--oc2-text-3); }
</style>
