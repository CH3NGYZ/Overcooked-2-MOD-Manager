<script setup lang="ts">
import { computed } from 'vue'
import { NButton, NInput, NTable } from 'naive-ui'
import { t } from '../../i18n'
import { RELATION_KINDS, type RelationTables, type RelationRow } from '../../mapPublishRelations'
import type { RelationKind } from '../../types'
const props = defineProps<{ tables: RelationTables; prefix: string; disabled: boolean }>()
const emit = defineEmits<{ update: [kind: RelationKind, rows: RelationRow[]] }>()
const kinds = computed(() => RELATION_KINDS.map(id => ({ id, label:
  id === 'requires' ? t('mapPublish.requires') : id === 'compatible' ? t('mapPublish.compatible') : t('mapPublish.conflicts') })))
function update(kind: RelationKind, index: number, field: 'guid' | 'range', value: string) {
  emit('update', kind, props.tables[kind].map((row, i) => i === index ? { ...row, [field]: value } : row))
}
</script>
<template>
  <section v-for="kind in kinds" :key="kind.id" class="relation-table-group" :data-relation-kind="kind.id" :aria-label="kind.label">
    <div class="relation-table-head">
      <h4>{{ kind.label }} <span class="section-count">{{ tables[kind.id].length }}</span></h4>
      <n-button size="small" secondary :disabled="disabled" @click="emit('update', kind.id, [...tables[kind.id], { guid: '', range: '' }])">{{ t('mapPublish.relationAddRow') }}</n-button>
    </div>
    <div class="publish-table-scroll">
      <n-table class="metadata-table relations-table" size="small" :single-line="false" :aria-label="kind.label">
        <thead><tr><th scope="col">GUID</th><th scope="col">{{ t('mapPublish.versionRangeColumn') }}</th><th scope="col">{{ t('mapPublish.actionsColumn') }}</th></tr></thead>
        <tbody>
          <tr v-for="(row, index) in tables[kind.id]" :key="index">
            <td><div :data-validation-field="`${prefix}-${kind.id}-${index}-guid`">
              <n-input :value="row.guid" size="small" :input-props="{ 'aria-label': `${kind.label} · GUID` }"
                :placeholder="t('mapPublish.relationGuidPlaceholder')" :disabled="disabled" @update:value="update(kind.id, index, 'guid', $event)" />
            </div></td>
            <td><div :data-validation-field="`${prefix}-${kind.id}-${index}-range`">
              <n-input :value="row.range" size="small" :input-props="{ 'aria-label': `${kind.label} · ${t('mapPublish.versionRangeColumn')}` }"
                :placeholder="t('mapPublish.relationRangePlaceholder')" :disabled="disabled" @update:value="update(kind.id, index, 'range', $event)" />
            </div></td>
            <td><n-button size="small" quaternary type="error" :disabled="disabled" @click="emit('update', kind.id, tables[kind.id].filter((_, i) => i !== index))">{{ t('common.action.delete') }}</n-button></td>
          </tr>
          <tr v-if="!tables[kind.id].length"><td colspan="3" class="muted">{{ t('mapPublish.relationEmpty') }}</td></tr>
        </tbody>
      </n-table>
    </div>
  </section>
</template>
