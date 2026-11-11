<script setup lang="ts">
import { NTag } from 'naive-ui'
import { t } from '../../i18n'
import { tagColorFor } from '../../theme'
import type { ChefFilters } from '../../diychef/projection'

defineProps<{ counts: Record<'native' | 'custom' | 'enabled' | 'disabled', number> }>()
const sources = defineModel<ChefFilters['sources']>('sources', { required: true })
const statuses = defineModel<ChefFilters['statuses']>('statuses', { required: true })
const sourceKeys = ['custom', 'native'] as const
const statusKeys = ['enabled', 'disabled'] as const
function toggleSource(key: typeof sourceKeys[number]) {
  sources.value = sources.value.includes(key) ? sources.value.filter(item => item !== key) : [...sources.value, key]
}
function toggleStatus(key: typeof statusKeys[number]) {
  statuses.value = statuses.value.includes(key) ? statuses.value.filter(item => item !== key) : [...statuses.value, key]
}
</script>

<template>
  <span class="header-tag-group" :aria-label="t('diychef.sourceFilter')">
    <n-tag v-for="key in sourceKeys" :key="key" size="small" :bordered="false"
      class="state-tag state-tag--link" :class="{ 'state-tag--active': sources.includes(key) }"
      :color="tagColorFor('muted')" role="button" tabindex="0" :aria-pressed="sources.includes(key)"
      :data-chef-filter="key" :title="t('common.filterByStatus')" @click="toggleSource(key)"
      @keydown.enter.prevent="toggleSource(key)" @keydown.space.prevent="toggleSource(key)">
      {{ counts[key] }} {{ t(`diychef.${key}`) }}
    </n-tag>
  </span>
  <span class="header-tag-divider" />
  <span class="header-tag-group" :aria-label="t('diychef.statusFilter')">
    <n-tag v-for="key in statusKeys" :key="key" size="small" :bordered="false"
      class="state-tag state-tag--link" :class="{ 'state-tag--active': statuses.includes(key) }"
      :color="tagColorFor(key === 'enabled' ? 'accent' : 'muted')" role="button" tabindex="0" :aria-pressed="statuses.includes(key)"
      :data-chef-filter="key" :title="t('common.filterByStatus')" @click="toggleStatus(key)"
      @keydown.enter.prevent="toggleStatus(key)" @keydown.space.prevent="toggleStatus(key)">
      {{ counts[key] }} {{ t(`diychef.${key}`) }}
    </n-tag>
  </span>
</template>
