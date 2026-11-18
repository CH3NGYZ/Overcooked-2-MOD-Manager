<script setup lang="ts">
import { computed } from 'vue'
import { NProgress, NTag } from 'naive-ui'
import DownloadVolumes from '../components/DownloadVolumes.vue'
import { formatFileSize } from '../repositoryAssets'
import { t } from '../i18n'
import type { ChangePlanRow } from './presentation'

const { row, processing } = defineProps<{ row: ChangePlanRow; processing: boolean }>()
const download = computed(() => row.item?.download)
const actionKinds = computed(() => [...new Set(row.actions.map(action => action.kind))])
const percentage = computed(() => download.value?.total
  ? Math.max(0, Math.min(100, Math.floor(download.value.downloaded / download.value.total * 100))) : null)
const stageText = computed(() => {
  if (!row.item) return t('modChanges.item.queued')
  if (row.item.stage === 'downloading') {
    if (download.value?.phase === 'connecting') return t('tools.connectingToDownloadSource')
    if (download.value?.phase === 'switching') return t('tools.switchingToAFasterSource')
    if (download.value?.phase === 'merging') return t('tools.mergingVolumes')
  }
  return t(`modChanges.item.${row.item.stage}`)
})
const speedText = computed(() => download.value && download.value.speed_bytes > 0
  ? `${formatFileSize(download.value.speed_bytes)}/s` : '')
const downloadSize = computed(() => {
  const value = download.value
  if (!value) return ''
  return `${value.downloaded ? formatFileSize(value.downloaded) : '0 B'} / ${formatFileSize(value.total)}`
})
</script>

<template>
  <article class="plan-item" :class="{ 'is-active': row.item?.stage === 'downloading' || row.item?.stage === 'verifying' }" :data-change-node="row.nodeId">
    <div class="item-heading">
      <span v-if="actionKinds.length" class="item-kinds"><NTag v-for="kind in actionKinds" :key="kind" :type="kind === 'disable' ? 'warning' : 'info'" size="small">{{ t(`modChanges.action.${kind}`) }}</NTag></span>
      <strong>{{ row.label || t('modChanges.unnamedItem') }}</strong>
      <span v-if="row.action?.from_tag || row.action?.to_tag || row.item?.tag" class="item-version">
        <template v-if="row.action?.from_tag">{{ row.action.from_tag }}<span aria-hidden="true"> → </span></template>{{ row.item?.tag || row.action?.to_tag || '—' }}
      </span>
    </div>
    <div v-if="row.item || processing" class="item-stage" :class="{ 'is-ready': row.item?.stage === 'ready' }" role="status">{{ stageText }}</div>
    <div v-if="row.item?.stage === 'downloading' && download" class="item-download">
      <div class="item-progress-row">
        <NProgress type="line" :percentage="percentage ?? 100" :processing="percentage === null" :show-indicator="false" :height="5" />
        <span v-if="percentage !== null" class="item-percent">{{ percentage }}%</span>
      </div>
      <div class="item-download-meta"><span>{{ downloadSize }}</span><span>{{ speedText }}</span></div>
      <div v-if="download.route" class="item-route">{{ t(download.route === 'proxy' ? 'common.proxy' : 'common.direct') }}</div>
      <DownloadVolumes v-if="download.volumes" :volumes="download.volumes" />
    </div>
  </article>
</template>

<style scoped>
.plan-item { padding: 12px 14px; border: 1px solid var(--oc2-row-line); border-radius: 10px; min-width: 0; }
.plan-item.is-active { background: var(--oc2-sunken); }
.item-heading { display: flex; align-items: center; gap: 10px; }
.item-kinds { display: flex; flex: 0 1 auto; flex-wrap: wrap; gap: 4px; max-width: 30%; }
.item-heading strong { flex: 1; min-width: 0; overflow-wrap: anywhere; line-height: 1.55; }
.item-stage { margin-top: 5px; text-align: right; font-size: 12px; color: var(--oc2-text-3); line-height: 1.8; overflow-wrap: anywhere; }
.item-stage.is-ready { color: var(--oc2-accent); }
.item-version { flex: 0 0 auto; color: var(--oc2-text-2); font-size: 13px; font-variant-numeric: tabular-nums; white-space: nowrap; }
.item-version > span { padding: 0 6px; color: var(--oc2-text-3); }
.item-download { margin-top: 10px; }
.item-progress-row { display: flex; gap: 12px; align-items: center; }
.item-progress-row > .n-progress { flex: 1; min-width: 0; }
.item-percent { min-width: 34px; font-size: 12px; text-align: right; }
.item-download-meta { display: flex; justify-content: space-between; flex-wrap: wrap; gap: 4px 12px; margin-top: 6px; font-size: 12px; color: var(--oc2-text-3); font-variant-numeric: tabular-nums; }
.item-route { margin-top: 3px; font-size: 11px; color: var(--oc2-text-3); }
</style>
