<script setup lang="ts">
import { NButton } from 'naive-ui'
import { t } from '../../i18n'
import AppModal from '../AppModal.vue'

defineProps<{ show: boolean; canSave: boolean }>()
const emit = defineEmits<{ stay: []; discard: []; save: [] }>()
</script>

<template>
  <AppModal :show="show" :title="t('diychef.leaveTitle')" max-width="min(520px, 94vw)" :mask-closable="false" @update:show="emit('stay')">
    <p>{{ t('diychef.leaveHint') }}</p>
    <template #footer>
      <n-button size="small" @click="emit('stay')">{{ t('diychef.stay') }}</n-button>
      <n-button size="small" @click="emit('discard')">{{ t('diychef.discard') }}</n-button>
      <n-button size="small" type="primary" :disabled="!canSave" @click="emit('save')">{{ t('diychef.saveAndLeave') }}</n-button>
    </template>
  </AppModal>
</template>

<style scoped>
p { margin: 0; color: var(--oc2-text-2); font-size: 13px; }
</style>
