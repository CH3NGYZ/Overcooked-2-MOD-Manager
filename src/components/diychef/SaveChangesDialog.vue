<script setup lang="ts">
import { NButton } from 'naive-ui'
import { t } from '../../i18n'
import type { ChefChange } from '../../diychef/types'
import AppModal from '../AppModal.vue'

defineProps<{ show: boolean; changes: ChefChange[] }>()
const emit = defineEmits<{ resolve: [value: boolean] }>()
</script>

<template>
  <AppModal :show="show" :title="t('diychef.changesTitle')" max-width="min(720px, 94vw)" :mask-closable="false" @update:show="emit('resolve', false)">
    <p class="chef-change-hint">{{ t('diychef.changesHint') }}</p>
    <div v-for="change in changes" :key="change.id" class="chef-change">
      <div><span>{{ t('diychef.before') }}</span><pre>{{ change.before || '∅' }}</pre></div>
      <div><span>{{ t('diychef.after') }}</span><pre>{{ change.after || '∅' }}</pre></div>
    </div>
    <template #footer>
      <n-button size="small" @click="emit('resolve', false)">{{ t('diychef.cancel') }}</n-button>
      <n-button size="small" type="primary" @click="emit('resolve', true)">{{ t('diychef.confirmSave') }}</n-button>
    </template>
  </AppModal>
</template>

<style scoped>
.chef-change-hint { margin: 0 0 8px; color: var(--oc2-text-3); font-size: 13px; }
.chef-change { display: grid; grid-template-columns: 1fr 1fr; gap: 10px; padding: 12px; background: var(--oc2-row); border: 1px solid var(--oc2-row-line); border-radius: 8px; }
.chef-change > div { min-width: 0; }
.chef-change span { color: var(--oc2-text-3); font-size: 11px; }
.chef-change pre { margin: 6px 0 0; white-space: pre-wrap; overflow-wrap: anywhere; font-size: 12px; }
</style>
