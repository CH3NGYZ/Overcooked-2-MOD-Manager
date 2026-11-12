<script setup lang="ts">
import { computed } from 'vue'
import { NButton, NInput, NTable, NTabPane, NTabs, NTag } from 'naive-ui'
import { t } from '../../i18n'
import { I18N_FIELD_ORDER, MAP_I18N_LANGUAGES, MAP_I18N_LANGUAGE_LABELS, REQUIRED_I18N_LANGUAGES, extraLanguages, i18nTableOf, type I18nFieldId } from '../../mapPublishI18n'
import { RELATION_KINDS } from '../../mapPublishRelations'
import PublishRelationTables from './PublishRelationTables.vue'
import type { MapPublishEditor } from '../../composables/useMapPublishEditor'
const props = defineProps<{ editor: MapPublishEditor }>()
const { metadataDirty, busyAny, stepBusy, preparing, saveDocument } = props.editor
const { draft, setI18nValue } = props.editor.document
const { metadataSection } = props.editor.validation
const relationCount = computed(() => RELATION_KINDS.reduce((count, kind) => count + draft[kind].length, 0))
const keptLanguages = computed(() => [...new Set(I18N_FIELD_ORDER.flatMap(field => extraLanguages(draft[field.id])))])
function isRequired(field: I18nFieldId, language: string) {
  return (REQUIRED_I18N_LANGUAGES as readonly string[]).includes(language) && Object.keys(i18nTableOf(draft[field])).length > 0
}
</script>
<template>
  <div class="panel document-card">
    <div class="panel-head">
      <h3 class="sub-title">{{ t('mapPublish.documentCardTitle') }}</h3>
      <n-tag v-if="metadataDirty" size="small" type="warning" :bordered="false">{{ t('mapPublish.unsaved') }}</n-tag>
    </div>
    <n-tabs v-model:value="metadataSection" class="metadata-tabs" type="segment" size="small" :animated="false">
      <n-tab-pane name="i18n" :disabled="busyAny"><template #tab>{{ t('mapPublish.i18nSettings') }}</template></n-tab-pane>
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

    <section v-else class="metadata-section" :aria-label="t('mapPublish.relationsSection')">
      <PublishRelationTables :tables="draft" prefix="relation-default" :disabled="busyAny" @update="(kind, rows) => draft[kind] = rows" />
    </section>
    <div class="metadata-footer">
      <n-button size="small" type="primary" :loading="stepBusy === 'metadata'" :disabled="busyAny" @click="() => saveDocument()">{{ preparing ? t('mapPublish.continueToUpload') : t('mapPublish.saveMetadata') }}</n-button>
    </div>
  </div>
</template>
