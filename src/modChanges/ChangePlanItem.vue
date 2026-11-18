<script setup lang="ts">
import { computed } from 'vue'
import { NTag } from 'naive-ui'
import { t } from '../i18n'
import type { ChangePlanRow } from './presentation'

const { row, processing } = defineProps<{ row: ChangePlanRow; processing: boolean }>()
const actionKinds = computed(() => [...new Set(row.actions.map(action => action.kind))])
const stageText = computed(() => t(`modChanges.item.${row.item?.stage ?? 'queued'}`))
</script>

<template>
  <article class="plan-item" :class="{ 'is-active': row.item?.stage === 'downloading' || row.item?.stage === 'verifying' }" :data-change-node="row.nodeId">
    <div class="item-heading">
      <span v-if="actionKinds.length" class="item-kinds"><NTag v-for="kind in actionKinds" :key="kind" :type="kind === 'disable' ? 'warning' : 'info'" size="small">{{ t(`modChanges.action.${kind}`) }}</NTag></span>
      <strong>{{ row.label || t('modChanges.unnamedItem') }}</strong>
      <span v-if="row.action?.from_tag || row.action?.to_tag || row.item?.tag" class="item-version">
        <template v-if="row.action?.from_tag">{{ row.action.from_tag }}<span aria-hidden="true"> → </span></template>{{ row.item?.tag || row.action?.to_tag || '—' }}
      </span>
      <span v-if="row.item || processing" class="item-stage" :class="{ 'is-ready': row.item?.stage === 'ready' }" role="status"><span v-if="row.item?.stage === 'ready'" aria-hidden="true">✓</span>{{ stageText }}</span>
    </div>
  </article>
</template>

<style scoped>
.plan-item { padding: 12px 14px; border: 1px solid var(--oc2-row-line); border-radius: 10px; min-width: 0; }
.plan-item.is-active { background: var(--oc2-sunken); }
.item-heading { display: flex; align-items: center; gap: 10px; }
.item-kinds { display: flex; flex: 0 1 auto; flex-wrap: wrap; gap: 4px; max-width: 30%; }
.item-heading strong { flex: 1; min-width: 0; overflow-wrap: anywhere; line-height: 1.55; }
.item-stage { display: inline-flex; flex: 0 0 auto; align-items: center; gap: 4px; font-size: 12px; color: var(--oc2-text-3); line-height: 1.8; white-space: nowrap; }
.item-stage.is-ready { color: var(--oc2-accent); }
.item-version { flex: 0 0 auto; color: var(--oc2-text-2); font-size: 13px; font-variant-numeric: tabular-nums; white-space: nowrap; }
.item-version > span { padding: 0 6px; color: var(--oc2-text-3); }
</style>
