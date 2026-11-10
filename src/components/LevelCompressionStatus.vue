<script setup lang="ts">
import { computed } from 'vue'
import { NPopover, NProgress } from 'naive-ui'
import { CubeOutline } from '@vicons/ionicons5'
import { useLevelCompression } from '../composables/useLevelCompression'
import { t } from '../i18n'

const { compression } = useLevelCompression()
const busy = computed(() => !!compression.value?.running)
const percent = computed(() => {
  const p = compression.value
  return p?.total ? Math.min(100, Math.round(p.done / p.total * 100)) : 0
})
</script>

<template>
  <n-popover trigger="hover" placement="bottom" :z-index="5000" :show-arrow="false">
    <template #trigger>
      <button class="compression-button" :class="{ busy }" type="button"
        :aria-label="t('levelPreview.conversionTitle')" data-testid="level-compression-status">
        <CubeOutline width="17" height="17" />
        <span v-if="busy" class="activity-dot" />
      </button>
    </template>
    <div class="compression-panel">
      <strong>{{ t('levelPreview.conversionTitle') }}</strong>
        <table class="progress-table" :aria-label="t('levelPreview.conversionTitle')">
          <tbody>
            <tr><th scope="row">{{ t('levelPreview.conversionStatus') }}</th><td>{{ busy ? t('levelPreview.conversionActive') : t('levelPreview.conversionIdle') }}</td></tr>
            <tr><th scope="row">{{ t('levelPreview.conversionSets') }}</th><td>{{ compression?.sets_done ?? 0 }}/{{ compression?.sets_total ?? 0 }}</td></tr>
            <tr><th scope="row">{{ t('levelPreview.conversionFiles') }}</th><td>{{ compression?.files_done ?? 0 }}/{{ compression?.files_total ?? 0 }}</td></tr>
            <tr><th scope="row">{{ t('levelPreview.conversionTotal') }}</th><td>{{ compression?.done ?? 0 }}/{{ compression?.total ?? 0 }}</td></tr>
            <tr><td colspan="2"><n-progress type="line" :percentage="percent" :show-indicator="false" :height="4" /></td></tr>
          </tbody>
        </table>
        <small v-if="compression?.failed">{{ t('levelPreview.conversionFailed', { count: compression.failed }) }}</small>
    </div>
  </n-popover>
</template>

<style scoped>
.compression-button { position: relative; display: grid; place-items: center; width: 32px; height: 32px; padding: 0; border-radius: 9px; border: 1px solid var(--oc2-rail-line); background: var(--oc2-card); color: var(--oc2-text-3); cursor: pointer; transition: color .16s, background-color .16s; }
.compression-button:hover, .compression-button:focus-visible, .compression-button.busy { color: var(--oc2-accent); }
.compression-button:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
.busy svg { animation: conversion-orbit 3s linear infinite; }
.activity-dot { position: absolute; right: 4px; bottom: 4px; width: 4px; height: 4px; border-radius: 50%; background: var(--oc2-accent); }
.compression-panel { display: flex; flex-direction: column; gap: 9px; width: 272px; color: var(--oc2-text-2); }
.progress-table { width: 100%; border-collapse: collapse; font-size: 12px; }
.progress-table th, .progress-table td { padding: 8px 10px; border: 1px solid var(--oc2-row-line); }
.progress-table th { text-align: left; font-weight: 400; background: var(--oc2-sunken); }
.progress-table td { text-align: right; font-variant-numeric: tabular-nums; white-space: nowrap; }
small { color: var(--oc2-text-3); }
@keyframes conversion-orbit { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) { .busy svg { animation: none; } }
</style>
