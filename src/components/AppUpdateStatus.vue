<script setup lang="ts">
import { computed } from 'vue'
import { NButton, NTag } from 'naive-ui'
import { appUpdateState } from '../appUpdateState'
import { downloadPercent } from '../downloadPresentation'
import { t } from '../i18n'

const emit = defineEmits<{ 'apply-update': [] }>()
const statusText = computed(() => {
  const { stage, update, progress } = appUpdateState
  if (!update?.background_install) return ''
  if (stage === 'applying' || stage === 'handoff') return t('update.applyingUpdate')
  if (stage !== 'downloading') return ''
  if (progress?.finished || progress?.phase === 'finished') return t('update.verifyingUpdate')
  const percent = progress ? downloadPercent(progress) : null
  return percent === null ? t('update.downloadingInstaller')
    : t('update.downloadPercent', { percent: Math.floor(percent) })
})
</script>

<template>
  <n-button v-if="appUpdateState.stage === 'ready'" class="app-update-status" type="primary" tertiary
    size="tiny" @click="emit('apply-update')">
    {{ t('update.applyNow') }}
  </n-button>
  <n-tag v-else-if="statusText" class="app-update-status" type="info" size="small" :bordered="false" role="status">
    {{ statusText }}
  </n-tag>
</template>

<style scoped>
.app-update-status { flex: 0 0 auto; align-self: center; white-space: nowrap; font-variant-numeric: tabular-nums; }
</style>
