<script setup lang="ts">
import { NButton, NInput } from 'naive-ui'
import { t } from '../../i18n'
import type { MapPublishEditor } from '../../composables/useMapPublishEditor'
const props = defineProps<{ editor: MapPublishEditor }>()
const { form, workingRelease, busyAny, stepBusy, saveReleaseStep } = props.editor
const allowTag = (value: string) => /^[A-Za-z0-9]*$/.test(value)
</script>

<template>
  <div class="panel release-step">
    <h3 class="sub-title">{{ t('mapPublish.stepReleaseTitle') }}</h3>
    <div class="form-grid">
      <div class="field span-2" data-validation-field="tag">
        <label class="field-label" for="publish-tag">{{ t('mapPublish.tagLabel') }}
          <span v-if="!workingRelease" class="required-mark" aria-hidden="true">*</span>
        </label>
        <code v-if="workingRelease" class="mono rule-text">{{ workingRelease.tag }}</code>
        <n-input v-else v-model:value="form.tag" size="small" :allow-input="allowTag"
          :input-props="{ id: 'publish-tag', 'aria-required': true }"
          :placeholder="t('mapPublish.tagPlaceholder')" :disabled="busyAny" />
      </div>
      <div class="field span-2">
        <label class="field-label" for="publish-body">{{ t('mapPublish.bodyLabel') }}</label>
        <n-input v-model:value="form.body" type="textarea" :autosize="{ minRows: 4, maxRows: 12 }"
          :input-props="{ id: 'publish-body' }" :disabled="busyAny" />
      </div>
    </div>
    <div class="actions">
      <n-button size="small" type="primary" :loading="stepBusy === 'release'" :disabled="busyAny" @click="saveReleaseStep">
        {{ workingRelease ? t('mapPublish.saveAndNext') : t('mapPublish.createRelease') }}
      </n-button>
    </div>
  </div>
</template>
