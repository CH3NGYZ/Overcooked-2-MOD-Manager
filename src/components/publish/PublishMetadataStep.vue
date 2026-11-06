<script setup lang="ts">
import { computed } from 'vue'
import { NButton, NInput, NSelect, NTable, NTabPane, NTabs, NTag } from 'naive-ui'
import { t } from '../../i18n'
import VersionOrderTable from './VersionOrderTable.vue'
import { I18N_FIELD_ORDER, MAP_I18N_LANGUAGES, MAP_I18N_LANGUAGE_LABELS, REQUIRED_I18N_LANGUAGES, extraLanguages, i18nTableOf, type I18nFieldId } from '../../mapPublishI18n'
import { RELATION_KINDS } from '../../mapPublishRelations'
import PublishRelationTables from './PublishRelationTables.vue'
import type { MapPublishEditor } from '../../composables/useMapPublishEditor'
const props = defineProps<{ editor: MapPublishEditor }>()
const emit = defineEmits<{ remove: [name: string] }>()
const { dirty, busyAny, awaitingMetadata, stepBusy, assetRows, assetNameOf, saveDocument } = props.editor
const { draft, setI18nValue } = props.editor.document
const { metadataSection } = props.editor.validation
const relationCount = computed(() => [draft, ...draft.releaseRelations].reduce((total, tables) =>
  total + RELATION_KINDS.reduce((count, kind) => count + tables[kind].length, 0), 0))
const versionOptions = computed(() => [...assetRows.value].reverse().map(row => ({ label: row.mapsetVersion, value: `==${row.mapsetVersion}` })))
const keptLanguages = computed(() => [...new Set(I18N_FIELD_ORDER.flatMap(field => extraLanguages(draft[field.id])))])
function isRequired(field: I18nFieldId, language: string) {
  return (REQUIRED_I18N_LANGUAGES as readonly string[]).includes(language) && Object.keys(i18nTableOf(draft[field])).length > 0
}
function addOverride() {
  // 覆盖是整表替换，先复制默认关系，作者再按该版本增删，避免无意漏掉原有依赖。
  draft.releaseRelations.push({ version: '',
    requires: draft.requires.map(row => ({ ...row })),
    compatible: draft.compatible.map(row => ({ ...row })),
    conflicts: draft.conflicts.map(row => ({ ...row })),
  })
}
</script>
<template>
  <div class="panel document-card">
    <div class="panel-head">
      <h3 class="sub-title">{{ t('mapPublish.documentCardTitle') }}</h3>
      <n-tag v-if="dirty" size="small" type="warning" :bordered="false">{{ t('mapPublish.unsaved') }}</n-tag>
    </div>
    <n-tabs v-model:value="metadataSection" class="metadata-tabs" type="segment" size="small" :animated="false">
      <n-tab-pane name="i18n" :disabled="busyAny"><template #tab>{{ t('mapPublish.i18nSettings') }}</template></n-tab-pane>
      <n-tab-pane name="versions" :disabled="busyAny"><template #tab>{{ t('mapPublish.versionOrderSection') }} <span class="section-count">{{ assetRows.length }}</span></template></n-tab-pane>
      <n-tab-pane name="relations" :disabled="busyAny"><template #tab>{{ t('mapPublish.relationsSection') }} <span class="section-count">{{ relationCount }}</span></template></n-tab-pane>
    </n-tabs>

    <!-- 表格由本组件直接渲染，不把草稿更新交给组件库缓存的空面板插槽。 -->
    <section v-if="metadataSection === 'i18n'" class="metadata-section" :aria-label="t('mapPublish.i18nSettings')">
      <p class="hint no-top">{{ t('mapPublish.i18nHint') }}</p>
      <div class="publish-table-scroll">
        <n-table class="metadata-table i18n-table" size="small" :single-line="false" :aria-label="t('mapPublish.i18nSettings')">
          <thead><tr>
            <th scope="col">{{ t('mapPublish.languageColumn') }}</th>
            <th v-for="field in I18N_FIELD_ORDER" :key="field.id" scope="col">{{ t(field.labelKey) }}</th>
          </tr></thead>
          <tbody>
            <tr v-for="language in MAP_I18N_LANGUAGES" :key="language">
              <th scope="row">{{ MAP_I18N_LANGUAGE_LABELS[language] }}</th>
              <td v-for="field in I18N_FIELD_ORDER" :key="field.id">
                <div class="i18n-cell" :data-validation-field="`i18n-${field.id}-${language}`">
                  <span v-if="isRequired(field.id, language)" class="required-mark" aria-hidden="true">*</span>
                  <n-input size="small" :type="field.id === 'noteI18n' ? 'textarea' : 'text'"
                    :autosize="field.id === 'noteI18n' ? { minRows: 2, maxRows: 4 } : undefined"
                    :input-props="{ 'aria-label': `${MAP_I18N_LANGUAGE_LABELS[language]} · ${t(field.labelKey)}`, 'aria-required': isRequired(field.id, language) }"
                    :value="draft[field.id].values[language] ?? ''" :disabled="busyAny"
                    @update:value="value => setI18nValue(field.id, language, value)" />
                </div>
              </td>
            </tr>
          </tbody>
        </n-table>
      </div>
      <p v-if="keptLanguages.length" class="hint">{{ t('mapPublish.i18nExtraKept', { languages: keptLanguages.join(' / ') }) }}</p>
    </section>

    <section v-else-if="metadataSection === 'versions'" class="metadata-section" :aria-label="t('mapPublish.versionOrderSection')">
      <p class="hint no-top">{{ t('mapPublish.versionOrderHint') }}</p>
      <p v-if="!assetRows.length" class="hint">{{ t('mapPublish.versionMapEmpty') }}</p>
      <VersionOrderTable v-else v-model:rows="assetRows" :name-of="assetNameOf" :disabled="busyAny" :remove-disabled="awaitingMetadata"
        @remove="emit('remove', assetNameOf($event))" />
    </section>

    <section v-else class="metadata-section" :aria-label="t('mapPublish.relationsSection')">
      <p class="hint no-top">{{ t('mapPublish.relationsOptional') }}</p>
      <div class="relation-default">
        <div class="relation-scope-head"><h4>{{ t('mapPublish.defaultRelations') }}</h4></div>
        <PublishRelationTables :tables="draft" prefix="relation-default" :disabled="busyAny" @update="(kind, rows) => draft[kind] = rows" />
      </div>
      <div class="relation-override">
        <div class="relation-scope-head">
          <h4>{{ t('mapPublish.versionRelations') }}</h4>
          <n-button size="small" :disabled="busyAny" @click="addOverride">{{ t('mapPublish.addVersionRelations') }}</n-button>
        </div>
        <p class="hint">{{ t('mapPublish.versionRelationsHint') }}</p>
        <section v-for="(entry, index) in draft.releaseRelations" :key="index" class="relation-override" :data-relation-override="index">
          <div class="relation-scope-head">
            <h4>{{ t('mapPublish.assetVersionRange') }} {{ index + 1 }}</h4>
            <n-button size="small" quaternary type="error" :disabled="busyAny" @click="draft.releaseRelations.splice(index, 1)">{{ t('mapPublish.removeVersionRelations') }}</n-button>
          </div>
          <div class="relation-range" :data-validation-field="`relation-override-${index}-version`">
            <n-input v-model:value="entry.version" :disabled="busyAny" placeholder="==0.9 / >=0.9,<1.0" :input-props="{ 'aria-label': t('mapPublish.assetVersionRange') }" />
            <n-select :value="versionOptions.some(option => option.value === entry.version) ? entry.version : null" :options="versionOptions" :disabled="busyAny" :placeholder="t('mapPublish.selectAssetVersion')" @update:value="entry.version = $event" />
          </div>
          <PublishRelationTables :tables="entry" :prefix="`relation-override-${index}`" :disabled="busyAny" @update="(kind, rows) => entry[kind] = rows" />
        </section>
      </div>
    </section>
    <div class="metadata-footer">
      <n-button size="small" type="primary" :loading="stepBusy === 'metadata'" :disabled="busyAny" @click="() => saveDocument()">{{ t('mapPublish.saveMetadata') }}</n-button>
    </div>
  </div>
</template>
