<script setup lang="ts">
import { ref } from 'vue'
import { NButton, NTag } from 'naive-ui'
import { t } from '../../i18n'
import { assetRelationOverride, emptyRelationTables, relationIssue, withAssetRelations, type PublishRelationOverride } from '../../mapPublishRelations'
import { usePublishValidation } from '../../composables/usePublishValidation'
import { useNotify } from '../../useNotify'
import type { MapPublishEditor } from '../../composables/useMapPublishEditor'
import type { AssetVersionRow } from '../../mapPublishFlow'
import AppModal from '../AppModal.vue'
import VersionOrderTable from './VersionOrderTable.vue'
import PublishRelationTables from './PublishRelationTables.vue'
const props = defineProps<{ editor: MapPublishEditor }>()
const emit = defineEmits<{ remove: [name: string] }>()
const { assetRows, assetNameOf, busyAny, stepBusy, versionsDirty, saveDocument, selectStep } = props.editor
const { draft } = props.editor.document
const message = useNotify()
const relationRoot = ref<HTMLElement | null>(null)
const validation = usePublishValidation(relationRoot)
const relationsOpen = ref(false)
const editingVersion = ref('')
const editingRelations = ref<PublishRelationOverride | null>(null)
function editRelations(row: AssetVersionRow) {
  if (busyAny.value) return
  editingVersion.value = row.mapsetVersion
  editingRelations.value = assetRelationOverride(draft, draft.releaseRelations, row.mapsetVersion)
  relationsOpen.value = true
}
function closeRelations() {
  validation.clear()
  relationsOpen.value = false
}
function inheritRelations() {
  if (busyAny.value) return
  draft.releaseRelations = withAssetRelations(draft.releaseRelations, editingVersion.value, null)
  closeRelations()
}
async function confirmRelations() {
  if (busyAny.value || !editingRelations.value) return
  const entry = editingRelations.value
  // 没有任何生效关系时，打开空表再确认不应生成后端拒绝的空覆盖。
  const original = assetRelationOverride(draft, draft.releaseRelations, editingVersion.value)
  const empty = (value: PublishRelationOverride) => !value.requires.length && !value.compatible.length && !value.conflicts.length
  if (empty(entry) && empty(original)) { closeRelations(); return }
  const issue = relationIssue(emptyRelationTables(), [entry])
  if (issue) {
    message.warning(t('mapPublish.invalidRelations'))
    await validation.pulse(issue)
    return
  }
  draft.releaseRelations = withAssetRelations(draft.releaseRelations, editingVersion.value, entry)
  closeRelations()
}
</script>
<template>
  <div class="panel document-card versions-step">
    <div class="panel-head">
      <h3 class="sub-title">{{ t('mapPublish.stepVersionsTitle') }}</h3>
      <n-tag v-if="versionsDirty" size="small" type="warning" :bordered="false">{{ t('mapPublish.unsaved') }}</n-tag>
    </div>
    <p class="hint no-top">{{ t('mapPublish.versionOrderHint') }}</p>
    <p class="hint">{{ t('mapPublish.versionsReadOnly') }}</p>
    <p v-if="!assetRows.length" class="hint">{{ t('mapPublish.versionMapEmpty') }}</p>
    <VersionOrderTable v-else v-model:rows="assetRows" :name-of="assetNameOf" :disabled="busyAny"
      @remove="emit('remove', assetNameOf($event))" @relations="editRelations" />
    <div class="metadata-footer">
      <n-button size="small" :disabled="busyAny" @click="selectStep('asset')">{{ t('mapPublish.uploadNewVersion') }}</n-button>
      <n-button size="small" type="primary" :loading="stepBusy === 'versions'" :disabled="busyAny" @click="() => saveDocument('versions')">{{ t('mapPublish.saveVersions') }}</n-button>
    </div>
  </div>
  <AppModal :show="relationsOpen" :title="`${t('mapPublish.assetRelations')} · ${editingVersion}`"
    max-width="min(900px, 95vw)" card-class="publish-relations-modal" :closable="!busyAny"
    @update:show="value => { if (!value) closeRelations() }"
    @after-leave="() => { if (!relationsOpen) editingRelations = null }">
    <div ref="relationRoot" class="map-publish publish-relations-editor">
      <p class="hint no-top">{{ t('mapPublish.assetRelationsHint') }}</p>
      <div v-if="editingRelations" data-validation-field="relation-override-0-version" tabindex="-1">
        <PublishRelationTables :tables="editingRelations" prefix="relation-override-0" :disabled="busyAny"
          @update="(kind, rows) => { if (editingRelations) editingRelations[kind] = rows }" />
      </div>
    </div>
    <template #footer>
      <n-button size="small" :disabled="busyAny" @click="inheritRelations">{{ t('mapPublish.inheritRelations') }}</n-button>
      <n-button size="small" :disabled="busyAny" @click="closeRelations">{{ t('common.action.cancel') }}</n-button>
      <n-button size="small" type="primary" :disabled="busyAny" @click="confirmRelations">{{ t('common.action.ok') }}</n-button>
    </template>
  </AppModal>
</template>
