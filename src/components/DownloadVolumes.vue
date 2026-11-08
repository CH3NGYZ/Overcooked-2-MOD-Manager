<script setup lang="ts">
import { t } from '../i18n'
import type { DownloadProgress } from '../types'
import { formatFileSize } from '../repositoryAssets'

defineProps<{ volumes: NonNullable<DownloadProgress['volumes']> }>()
</script>

<template>
  <div v-if="volumes.length" class="dl-volumes">
    <p>{{ t('tools.volumeSummary', { done: volumes.filter(v => v.state === 'completed').length, total: volumes.length }) }}</p>
    <div v-for="(volume, index) in volumes" :key="volume.name" class="dl-volume">
      <span class="dl-volume-name">{{ index + 1 }}. {{ volume.name }}</span>
      <span>{{ t(`tools.volumeState.${volume.state}`) }} · {{ t('tools.volumeRetry', { count: volume.retry }) }}</span>
      <span>{{ formatFileSize(volume.progress.downloaded) }} / {{ formatFileSize(volume.progress.total) }}</span>
      <span v-if="volume.state === 'downloading'">
        {{ volume.progress.route === 'proxy' ? t('common.proxy') : t('common.direct') }}
        · {{ volume.progress.source }}
        · {{ t('tools.attempt', { current: volume.progress.attempt ?? 1, total: volume.progress.attempt_total ?? 1 }) }}
        · {{ t('tools.sourcesRacing', { count: volume.progress.parallel_sources ?? 0 }) }}
      </span>
    </div>
  </div>
</template>

<style scoped>
.dl-volumes { margin-top: 10px; max-height: 210px; overflow: auto; font-size: 11px; color: var(--oc2-text-3); }
.dl-volumes p { margin: 0 0 6px; }
.dl-volume { display: flex; flex-direction: column; gap: 3px; padding: 5px 0; }
.dl-volume-name { overflow-wrap: anywhere; color: var(--oc2-text-2); }
</style>
