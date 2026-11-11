<script setup lang="ts">
import { NButton } from 'naive-ui'
import { t } from '../../i18n'
import type { ChefTarget } from '../../diychef/types'

defineProps<{ candidates: ChefTarget[]; busy: boolean }>()
const emit = defineEmits<{ select: [id: string] }>()
</script>

<template>
  <section class="chef-target-picker">
    <h2>{{ t('diychef.chooseTarget') }}</h2>
    <p>{{ t('diychef.chooseTargetHint') }}</p>
    <n-button v-for="target in candidates" :key="target.id" class="chef-target-option" :disabled="busy" @click="emit('select', target.id)">
      <span class="chef-target-option-copy"><strong>DIYChef {{ target.version || t('diychef.unknownVersion') }}<small v-if="!target.enabled">{{ t('diychef.targetDisabled') }}</small></strong><span>{{ target.display_path }}</span></span>
    </n-button>
  </section>
</template>

<style scoped>
.chef-target-picker { max-width: 720px; display: flex; flex-direction: column; gap: 10px; }
h2 { margin: 0; font-size: 17px; }
p { margin: 0 0 6px; color: var(--oc2-text-3); }
.chef-target-option { width: 100%; height: auto; padding: 12px; justify-content: flex-start; }
.chef-target-option :deep(.n-button__content) { width: 100%; justify-content: flex-start; }
.chef-target-option-copy { display: flex; flex-direction: column; gap: 6px; width: 100%; text-align: left; white-space: normal; overflow-wrap: anywhere; }
strong { font-size: 13px; }
small { margin-left: 12px; font-weight: 400; color: var(--oc2-text-3); }
.chef-target-option-copy > span { font-size: 12px; color: var(--oc2-text-3); }
</style>
