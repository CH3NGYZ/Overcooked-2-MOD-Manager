<script setup lang="ts">
import { computed, shallowRef } from 'vue'
import { NInput } from 'naive-ui'
import { CheckmarkCircle, SearchOutline } from '@vicons/ionicons5'
import { t } from '../../i18n'
import { hatLabel } from '../../diychef/hatLabels'
import { isUsableResource } from '../../diychef/draft'
import type { ChefHat } from '../../diychef/types'

const props = defineProps<{ hats: ChefHat[]; selected: string | null }>()
const emit = defineEmits<{ select: [hatId: string] }>()
const query = shallowRef('')
const visible = computed(() => {
  const search = query.value.trim().toLocaleLowerCase()
  return props.hats.filter((hat) => !search || [hatLabel(hat), hat.display_name, hat.config_value].some((name) => name.toLocaleLowerCase().includes(search)))
})
</script>

<template>
  <div class="chef-hat-picker">
    <n-input v-model:value="query" size="small" :clearable="!!query" :placeholder="t('diychef.hatSearch')" :input-props="{ 'aria-label': t('diychef.hatSearch') }">
      <template #prefix><SearchOutline width="14" height="14" aria-hidden="true" /></template>
    </n-input>
    <div class="chef-hat-options" role="group" :aria-label="t('diychef.changeHat')">
      <button v-for="hat in visible" :key="hat.id" type="button" class="chef-hat-option" :class="{ selected: selected === hat.id }"
        :aria-pressed="selected === hat.id" :disabled="!isUsableResource(hat.resource_state)" @click="emit('select', hat.id)">
        <span>{{ hatLabel(hat) }}<small v-if="hat.kind === 'auto'">{{ t('diychef.hatAutoHint') }}</small><small v-else-if="!isUsableResource(hat.resource_state)">{{ t('diychef.invalidResource') }}</small></span>
        <CheckmarkCircle v-if="selected === hat.id" aria-hidden="true" />
      </button>
      <p v-if="!visible.length" class="chef-hat-empty">{{ t('diychef.noHats') }}</p>
    </div>
  </div>
</template>

<style scoped>
.chef-hat-picker { min-width: 0; display: flex; flex-direction: column; gap: 10px; }
.chef-hat-options { display: grid; gap: 6px; align-content: start; }
.chef-hat-option { display: flex; justify-content: space-between; align-items: center; gap: 10px; padding: 9px 11px; border: 1px solid var(--oc2-row-line); border-radius: 8px; background: var(--oc2-row); color: var(--oc2-text-1); font: inherit; font-size: 13px; text-align: left; cursor: pointer; }
.chef-hat-option > span { min-width: 0; overflow-wrap: anywhere; }
.chef-hat-option small { display: block; margin-top: 3px; font-size: 11px; color: var(--oc2-text-3); }
.chef-hat-option svg { width: 17px; height: 17px; flex: 0 0 17px; color: var(--oc2-accent); }
.chef-hat-option.selected { border-color: var(--oc2-accent); }
.chef-hat-option:disabled { opacity: .6; cursor: default; }
.chef-hat-option:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
.chef-hat-empty { color: var(--oc2-text-3); font-size: 12px; }
</style>
