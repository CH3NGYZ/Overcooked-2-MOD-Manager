<script setup lang="ts">
import { shallowRef } from 'vue'
import { NTabPane, NTabs, NTooltip } from 'naive-ui'
import { CheckmarkCircleOutline, CloseCircleOutline } from '@vicons/ionicons5'
import { t } from '../i18n'

const props = defineProps<{ value: '' | 'enable' | 'disable'; disabled: boolean }>()
const emit = defineEmits<{ change: [value: 'enable' | 'disable'] }>()
const focused = shallowRef('')
const choices = [
  { value: 'enable', label: 'overview.enableAll', icon: CheckmarkCircleOutline },
  { value: 'disable', label: 'overview.disableAll', icon: CloseCircleOutline },
] as const
function select(value: string | number) {
  if (!props.disabled && (value === 'enable' || value === 'disable')) emit('change', value)
}
function tabProps(value: 'enable' | 'disable', label: string) {
  return {
    role: 'button', tabindex: props.disabled ? -1 : 0,
    'aria-label': label, 'aria-pressed': props.value === value, 'aria-disabled': props.disabled,
    onFocus: () => { focused.value = value },
    onBlur: () => { focused.value = '' },
    onKeydown: (event: KeyboardEvent) => {
      if (event.key === 'Enter' || event.key === ' ') { event.preventDefault(); select(value) }
    },
  }
}
</script>

<template>
  <n-tabs type="segment" animated class="bulk-switch" :class="{ 'is-mixed': !value }"
    :value="value" @update:value="select">
    <n-tab-pane v-for="choice in choices" :key="choice.value" :name="choice.value" :disabled="disabled"
      :tab-props="tabProps(choice.value, t(choice.label))">
      <template #tab>
        <n-tooltip placement="top" :delay="150" :keep-alive-on-hover="false" :show="focused === choice.value ? true : undefined">
          <template #trigger>
            <span class="bulk-switch-icon"><component :is="choice.icon" width="18" height="18" aria-hidden="true" /></span>
          </template>
          {{ t(choice.label) }}
        </n-tooltip>
      </template>
    </n-tab-pane>
  </n-tabs>
</template>
