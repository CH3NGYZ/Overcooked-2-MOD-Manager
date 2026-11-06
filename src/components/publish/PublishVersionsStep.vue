<script setup lang="ts">
import { nextTick } from 'vue'
import { NButton, NInput, NTag } from 'naive-ui'
import { t } from '../../i18n'
import { assetRelationOverride } from '../../mapPublishRelations'
import type { MapPublishEditor } from '../../composables/useMapPublishEditor'
import type { AssetVersionRow } from '../../mapPublishFlow'
import VersionOrderTable from './VersionOrderTable.vue'
import PublishRelationTables from './PublishRelationTables.vue'
const props = defineProps<{ editor: MapPublishEditor }>()
const emit = defineEmits<{ remove: [name: string] }>()
const { assetRows, assetNameOf, busyAny, awaitingMetadata, stepBusy, versionsDirty, saveDocument, validation } = props.editor
const { draft } = props.editor.document
async function editRelations(row: AssetVersionRow) {
  const index = assetRelationOverride(draft, draft.releaseRelations, row.mapsetVersion)
  await nextTick()
  await validation.pulse(`relation-override-${index}-version`)
}
function addOverride() {
  draft.releaseRelations.push({ version: '',
    requires: draft.requires.map(row => ({ ...row })),
    compatible: draft.compatible.map(row => ({ ...row })),
    conflicts: draft.conflicts.map(row => ({ ...row })),
  })
}
</script>
<template>
  <div class="panel document-card versions-step">
    <div class="panel-head">
      <h3 class="sub-title">{{ t('mapPublish.stepVersionsTitle') }}</h3>
      <n-tag v-if="versionsDirty" size="small" type="warning" :bordered="false">{{ t('mapPublish.unsaved') }}</n-tag>
    </div>
    <p class="hint no-top">{{ t('mapPublish.versionOrderHint') }}</p>
    <p v-if="!assetRows.length" class="hint">{{ t('mapPublish.versionMapEmpty') }}</p>
    <VersionOrderTable v-else v-model:rows="assetRows" :name-of="assetNameOf" :disabled="busyAny" :remove-disabled="awaitingMetadata"
      @remove="emit('remove', assetNameOf($event))" @relations="editRelations" />
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
        </div>
        <PublishRelationTables :tables="entry" :prefix="`relation-override-${index}`" :disabled="busyAny" @update="(kind, rows) => entry[kind] = rows" />
      </section>
    </div>
    <div class="metadata-footer">
      <n-button size="small" type="primary" :loading="stepBusy === 'versions'" :disabled="busyAny" @click="() => saveDocument('versions')">{{ t('mapPublish.saveVersions') }}</n-button>
    </div>
  </div>
</template>
