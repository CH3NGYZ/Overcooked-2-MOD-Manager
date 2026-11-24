<script setup lang="ts">
import { CopyOutline, FolderOpenOutline } from '@vicons/ionicons5'
import HeaderIconAction from '../components/HeaderIconAction.vue'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import type { EvidenceRow } from './evidence'

defineProps<{ rows: readonly EvidenceRow[]; guid?: string; truncated?: boolean; names?: ReadonlyMap<string, string> }>()
const notify = useNotify()
const status = (enabled: boolean) => t(enabled ? 'common.state.enabled' : 'common.state.disabled')
async function visit(path: string, locate: boolean) {
  try {
    const result = await (locate ? api.openModFileLocation(path, true) : api.copyText(path))
    if (result.ok) notify.success(result.message)
    else notify.error(backendMessageOf(result.message))
  } catch (reason) { notify.error(backendMessageOf(reason)) }
}
</script>

<template>
  <section v-if="rows.length" class="change-evidence" data-change-evidence>
    <p v-if="guid" class="evidence-guid">GUID: <code>{{ guid }}</code></p>
    <ul :aria-label="t(guid ? 'modChanges.evidence.providers' : 'modChanges.evidence.paths')" role="list">
      <li v-for="(row, index) in rows" :key="index" :data-evidence-origin="row.provider?.origin">
        <div class="evidence-content">
          <div v-if="row.provider" class="evidence-meta">
            <strong>{{ t(`modChanges.evidence.${row.provider.origin}`) }}</strong>
            <span>{{ row.provider.version || t('modChanges.evidence.unknownVersion') }}</span>
            <span>{{ status(row.provider.enabled) }}</span>
            <span v-if="row.provider.installed_enabled !== undefined && row.provider.installed_enabled !== row.provider.enabled">
              {{ t('modChanges.evidence.diskState', { state: status(row.provider.installed_enabled) }) }}
            </span>
            <span v-if="row.provider.node_id && names?.get(row.provider.node_id)">{{ names.get(row.provider.node_id) }}</span>
            <span v-if="row.provider.source">{{ row.provider.source }}</span>
          </div>
          <code v-if="row.path" class="evidence-path" data-evidence-path>{{ row.path }}</code>
        </div>
        <div v-if="row.path" class="evidence-buttons">
          <HeaderIconAction v-if="row.locate" :label="t('modChanges.evidence.reveal')" :description="row.path" quaternary
            data-evidence-locate @click="visit(row.path!, true)"><FolderOpenOutline width="15" height="15" /></HeaderIconAction>
          <HeaderIconAction :label="t('modChanges.evidence.copyPath')" :description="row.path" quaternary
            data-evidence-copy @click="visit(row.path!, false)"><CopyOutline width="15" height="15" /></HeaderIconAction>
        </div>
      </li>
    </ul>
    <p v-if="truncated" class="evidence-guid">{{ t('modChanges.evidence.truncated') }}</p>
  </section>
</template>

<style scoped>
.change-evidence { min-width: 0; max-width: 100%; margin-block: 6px 0; text-align: start; }
.change-evidence ul { list-style: none; padding: 0; margin: 0; }
.change-evidence li { display: flex; align-items: center; gap: 6px; padding: 3px 0; }
.change-evidence li + li { border-top: 1px solid var(--oc2-row-line); }
.evidence-content { flex: 1; min-width: 0; overflow-wrap: anywhere; }
.evidence-path { display: block; overflow-wrap: anywhere; white-space: pre-wrap; }
.evidence-guid { margin: 2px 0; overflow-wrap: anywhere; }
.evidence-meta { display: flex; flex-wrap: wrap; align-items: baseline; gap: 2px 9px; color: var(--oc2-text-2); font-size: 12px; }
.evidence-buttons { flex: none; display: flex; }
</style>
