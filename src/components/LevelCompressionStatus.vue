<script setup lang="ts">
import { computed } from 'vue'
import { CubeOutline } from '@vicons/ionicons5'
import { useLevelCompression } from '../composables/useLevelCompression'
import { t } from '../i18n'
import TitleBarProgressTable from './TitleBarProgressTable.vue'
import TitleBarStatusPopover from './TitleBarStatusPopover.vue'

const { compression } = useLevelCompression()
const busy = computed(() => !!compression.value?.running)
const percent = computed(() => {
  const status = compression.value
  return status?.total ? Math.min(100, Math.round(status.done / status.total * 100)) : 0
})
const rows = computed(() => {
  const status = compression.value
  return [
    { label: t('levelPreview.conversionStatus'), value: t(busy.value ? 'levelPreview.conversionActive' : 'levelPreview.conversionIdle') },
    { label: t('levelPreview.conversionSets'), value: `${status?.sets_done ?? 0}/${status?.sets_total ?? 0}` },
    { label: t('levelPreview.conversionFiles'), value: `${status?.files_done ?? 0}/${status?.files_total ?? 0}` },
    { label: t('levelPreview.conversionTotal'), value: `${status?.done ?? 0}/${status?.total ?? 0}` },
  ]
})
</script>

<template>
  <TitleBarStatusPopover :label="t('levelPreview.conversionTitle')" :tone="busy ? 'activity' : undefined"
    :rotating="busy" data-testid="level-compression-status">
    <template #icon><CubeOutline width="17" height="17" aria-hidden="true" /></template>
    <strong>{{ t('levelPreview.conversionTitle') }}</strong>
    <TitleBarProgressTable :label="t('levelPreview.conversionTitle')" :rows="rows" :progress="percent" />
    <small v-if="compression?.failed">{{ t('levelPreview.conversionFailed', { count: compression.failed }) }}</small>
  </TitleBarStatusPopover>
</template>

<style scoped>
small { color: var(--oc2-text-3); }
</style>
