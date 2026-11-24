<script setup lang="ts">
import { t } from '../i18n'
import type { DownloadProgress } from '../types'
import { downloadAmount, downloadRate, downloadRoute } from '../downloadPresentation'
import { computed, shallowRef } from 'vue'
import type { DeepReadonly } from 'vue'
import DownloadRace from './DownloadRace.vue'

const props = defineProps<{ volumes: DeepReadonly<NonNullable<DownloadProgress['volumes']>>; interactive: boolean }>()
const open = shallowRef(false)
const complete = computed(() => props.volumes.filter(volume => volume.state === 'completed').length)
const openVolumes = computed(() => open.value || props.volumes.some(volume => volume.progress.phase === 'locked_failed'))
</script>

<template>
  <details v-if="volumes.length" class="dl-volumes" :open="openVolumes" @toggle="open = ($event.currentTarget as HTMLDetailsElement).open">
    <summary data-download-details tabindex="0">{{ t('tools.volumeSummary', { done: complete, total: volumes.length }) }}</summary>
    <ul v-if="openVolumes" class="dl-volume-list">
      <li v-for="volume in volumes" :key="volume.name" class="dl-volume">
        <div class="dl-volume-heading">
          <span class="dl-volume-name">{{ volume.name }}</span>
          <span>{{ t(volume.progress.phase === 'locked_failed' ? 'tools.downloadRace.lockedFailed'
            : volume.state === 'downloading' && volume.progress.phase === 'connecting' ? 'tools.downloadRace.connecting'
            : volume.state === 'downloading' && volume.progress.phase === 'verifying' ? 'tools.downloadRace.verifying'
            : `tools.volumeState.${volume.state}`) }}</span>
        </div>
        <div class="dl-volume-meta">
          <span>{{ downloadAmount(volume.progress) }}</span>
          <span v-if="volume.state === 'downloading' && downloadRate(volume.progress)">{{ downloadRate(volume.progress) }}</span>
          <span v-if="volume.retry">{{ t('tools.volumeRetry', { count: volume.retry }) }}</span>
        </div>
        <DownloadRace v-if="volume.progress.race && (volume.state === 'downloading' || volume.progress.phase === 'locked_failed')"
          :key="volume.progress.race.id" :race="volume.progress.race" :interactive="interactive && !volume.progress.finished" />
        <div v-else-if="volume.state === 'downloading'" class="dl-volume-meta">
          <span v-if="downloadRoute(volume.progress)">{{ downloadRoute(volume.progress) }}</span>
        </div>
      </li>
    </ul>
  </details>
</template>

<style scoped>
.dl-volumes { min-width: 0; font-size: 12px; color: var(--oc2-text-2); }
.dl-volumes summary { width: fit-content; max-width: 100%; padding-block: 3px; cursor: pointer; }
.dl-volumes summary:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: -1px; border-radius: 4px; }
.dl-volume-list { margin: 3px 0 0; padding: 0; list-style: none; }
.dl-volume { display: grid; gap: 3px; padding: 6px 0; border-top: 1px solid var(--oc2-row-line); }
.dl-volume-heading, .dl-volume-meta { display: flex; flex-wrap: wrap; gap: 2px 8px; min-width: 0; overflow-wrap: anywhere; }
.dl-volume-heading { justify-content: space-between; }
.dl-volume-name { min-width: 0; font-weight: 600; color: var(--oc2-text-1); }
.dl-volume-meta { color: var(--oc2-text-3); font-variant-numeric: tabular-nums; }
</style>
