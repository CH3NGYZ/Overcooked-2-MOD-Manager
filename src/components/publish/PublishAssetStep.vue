<script setup lang="ts">
import { NButton, NProgress, NSpin, NTable } from 'naive-ui'
import { t } from '../../i18n'
import { formatFileSize } from '../../repositoryAssets'
import type { MapPublishEditor } from '../../composables/useMapPublishEditor'
import type { PublishUploadProgress } from '../../types'
const props = defineProps<{ editor: MapPublishEditor; dragActive: boolean; inspecting: boolean; uploadProgress: PublishUploadProgress | null }>()
const { pending, canonicalName, inspectionError, busyAny, stepBusy, selectingZip, repositoryName, form, pickPendingZip, uploadPending } = props.editor
const requiresText = (record: Record<string, string>) => Object.entries(record).map(([key, value]) => `${key} ${value}`).join(', ')
</script>

<template>
  <div class="panel publish-card">
    <div class="panel-head">
      <h3 class="sub-title">{{ selectingZip ? t('mapPublish.selectZipTitle') : t('mapPublish.stepAssetTitle') }}</h3>
    </div>
    <div class="drop-zone" :class="{ active: dragActive }" data-validation-field="zip" role="button"
      :tabindex="busyAny ? -1 : 0" :aria-disabled="busyAny"
      @click="pickPendingZip" @keydown.enter="pickPendingZip" @keydown.space.prevent="pickPendingZip">
      <p class="drop-title">{{ t('mapPublish.dropZone') }}<span class="required-mark" aria-hidden="true">*</span></p>
      <p v-if="pending.fileName" class="mono picked-zip">{{ pending.fileName }}</p>
    </div>
    <div v-if="inspecting" class="state-line"><n-spin size="small" /><span>{{ t('mapPublish.inspectingZip') }}</span></div>
    <div v-if="uploadProgress && uploadProgress.phase !== 'done'" class="publish-upload-progress">
      <span>{{ uploadProgress.phase === 'confirming' ? t('mapPublish.uploadConfirming') : t('mapPublish.uploading') }}</span>
      <n-progress type="line" :percentage="uploadProgress.total > 0 ? Math.round(uploadProgress.sent * 100 / uploadProgress.total) : 0" :show-indicator="false" />
      <span class="mono">{{ uploadProgress.total > 0 ? Math.round(uploadProgress.sent * 100 / uploadProgress.total) : 0 }}%</span>
    </div>
    <template v-if="pending.inspection">
      <n-table class="inspection-table" size="small" :single-line="false">
        <caption>{{ t('mapPublish.zipSectionInspection') }}</caption>
        <tbody>
          <tr v-if="!selectingZip"><th scope="row">{{ t('mapPublish.repositories') }}</th><td>{{ repositoryName }}</td></tr>
          <tr v-if="!selectingZip"><th scope="row">{{ t('mapPublish.tagLabel') }}</th><td>{{ form.tag || '—' }}</td></tr>
          <tr><th scope="row">{{ t('mapPublish.sizeColumn') }}</th><td>{{ formatFileSize(pending.inspection.file_size) }}</td></tr>
          <tr><th scope="row">{{ t('mapPublish.levelSetName') }}</th><td>{{ pending.inspection.level_set_name || '—' }}</td></tr>
          <tr><th scope="row">{{ t('mapPublish.levelSetNameZh') }}</th><td>{{ pending.inspection.level_set_name_zh || '—' }}</td></tr>
          <tr><th scope="row">{{ t('mapPublish.author') }}</th><td>{{ pending.inspection.author || '—' }}</td></tr>
          <tr><th scope="row">{{ t('mapPublish.inspectionInfoVersion') }}</th><td class="mono">{{ pending.inspection.info_version || '—' }}</td></tr>
          <tr><th scope="row">{{ t('mapPublish.inspectionMapsetVersion') }}</th><td class="mono">{{ pending.inspection.mapset_version || '—' }}</td></tr>
          <tr><th scope="row">{{ t('mapPublish.inspectionSetCount') }}</th><td>{{ pending.inspection.set_count }}</td></tr>
          <tr><th scope="row">{{ t('levelSet.levelCountLabel') }}</th><td>{{ pending.inspection.level_count }}</td></tr>
          <tr><th scope="row">{{ t('levelSet.fileCountLabel') }}</th><td>{{ pending.inspection.file_count }}</td></tr>
          <tr v-if="Object.keys(pending.inspection.requires).length"><th scope="row">{{ t('mapPublish.inspectionRequires') }}</th><td class="mono">{{ requiresText(pending.inspection.requires) }}</td></tr>
        </tbody>
      </n-table>
      <p v-if="inspectionError" class="inspection-error" role="alert">{{ inspectionError }}</p>
      <p v-else-if="pending.accepted" class="hint">{{ t('mapPublish.zipReadyHint') }}</p>
      <p v-if="pending.accepted && !selectingZip" class="hint mono">{{ t('mapPublish.zipNameWillUploadAs', { name: canonicalName }) }}</p>
    </template>
    <p v-if="!selectingZip" class="hint">{{ t('mapPublish.uploadConfirmationHint') }}</p>
    <div v-if="!selectingZip" class="actions">
      <n-button size="small" type="primary" :loading="stepBusy === 'asset'" :disabled="busyAny" @click="uploadPending">{{ t('mapPublish.confirmUpload') }}</n-button>
    </div>
  </div>
</template>
