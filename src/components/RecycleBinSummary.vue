<script setup lang="ts">
import { computed } from 'vue'
import { useRecycleBin } from '../composables/useRecycleBin'
import { t } from '../i18n'
import { formatFileSize } from '../repositoryAssets'

const { count, totalBytes, loaded } = useRecycleBin()
// An empty bin has a known size; release assets use a dash for unknown sizes.
const size = computed(() => !loaded.value ? '—' : totalBytes.value ? formatFileSize(totalBytes.value) : '0 B')
</script>

<template>
  <span class="recycle-summary" data-testid="recycle-bin-summary" aria-live="polite">
    <span>{{ loaded ? t('recycle.entryCount', { count }) : '—' }}</span>
    <span class="summary-separator" aria-hidden="true">·</span>
    <span :title="t('recycle.recordedFileSize')">{{ size }}</span>
  </span>
</template>

<style scoped>
.recycle-summary { display: inline-flex; align-items: center; gap: 8px; color: var(--oc2-text-3); font-size: 12px; font-variant-numeric: tabular-nums; white-space: nowrap; }
.summary-separator { color: var(--oc2-text-4); }
</style>
