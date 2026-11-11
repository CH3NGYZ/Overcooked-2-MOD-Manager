<script setup lang="ts">
import { computed } from 'vue'
import { NAlert, NButton } from 'naive-ui'
import { t } from '../../i18n'
import { translateBackendMessage } from '../../locale/backend'
import type { ChefDiagnostic, ChefEditorSnapshot } from '../../diychef/types'

const props = defineProps<{
  snapshot: ChefEditorSnapshot | null; error: string; gameRunning: boolean
  readOnly: boolean; stale: boolean; busy: boolean; diagnostics: ChefDiagnostic[]
}>()
const emit = defineEmits<{ reload: [] }>()
const generalDiagnostics = computed(() => props.diagnostics.filter((item) => !item.row_id))
function describe(item: ChefDiagnostic) {
  const chef = props.snapshot?.chefs.find((entry) => entry.id === item.chef_id)
  const message = translateBackendMessage(item.code)
  return chef ? `${chef.display_name}：${message}` : message
}
</script>

<template>
  <div class="chef-notices">
    <n-alert v-if="error" type="error" :show-icon="true">
      <div class="chef-notice-row"><span>{{ error }}</span><n-button size="tiny" :disabled="busy" @click="emit('reload')">{{ t('diychef.reload') }}</n-button></div>
    </n-alert>
    <n-alert v-if="stale" type="warning">{{ t('diychef.staleTarget') }}</n-alert>
    <n-alert v-if="snapshot && readOnly" type="warning">{{ t('diychef.readOnly') }}</n-alert>
    <n-alert v-if="gameRunning" type="info">{{ t('diychef.gameRunning') }}</n-alert>
    <n-alert v-if="snapshot?.source.state === 'missing'" type="info">{{ t('diychef.missingConfig') }}</n-alert>
    <n-alert v-if="snapshot && !snapshot.target.enabled" type="warning">{{ t('diychef.targetDisabled') }}</n-alert>
    <n-alert v-for="item in generalDiagnostics" :key="item.id" :type="item.save_blocking ? 'error' : 'warning'">{{ describe(item) }}</n-alert>
  </div>
</template>

<style scoped>
.chef-notices { display: flex; flex-direction: column; gap: 8px; }
.chef-notices:not(:empty) { margin-bottom: 14px; }
.chef-notices :deep(.n-alert) { font-size: 12px; }
.chef-notice-row { display: flex; gap: 12px; align-items: center; justify-content: space-between; }
.chef-notice-row > span { overflow-wrap: anywhere; }
.chef-notice-row :deep(.n-button) { flex: 0 0 auto; }
</style>
