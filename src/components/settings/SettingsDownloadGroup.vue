<script setup lang="ts">
import { computed, onBeforeUnmount, shallowRef } from 'vue'
import { NSelect } from 'naive-ui'
import { t } from '../../i18n'
import { api } from '../../services/tauri'
import { useDashboard } from '../../composables/useDashboardStore'
import { backendMessageOf } from '../../locale/backend'
import { demoActive } from '../../tutorial/demo'
import { useNotify } from '../../useNotify'
import downloadDefaults from '../../downloadDefaults.json' with { type: 'json' }
import type { PublicConfig } from '../../types'
import SettingsGroup from './SettingsGroup.vue'
import SettingsRow from './SettingsRow.vue'

const props = defineProps<{ config: PublicConfig | null; disabled: boolean }>()
const emit = defineEmits<{ saved: [config: PublicConfig]; saving: [value: boolean] }>()
const dashboard = useDashboard()
const message = useNotify()
const saving = shallowRef(false)
const disabled = computed(() => saving.value || props.disabled || !props.config || demoActive.value)
let disposed = false

const { timeout_min_seconds: min, timeout_max_seconds: max } = downloadDefaults
const secondOptions = computed(() => Array.from({ length: max - min + 1 }, (_, index) => {
  const value = index + min
  return { value, label: t('settings.downloadSeconds', { seconds: value }) }
}))
const rows = [
  { key: 'api_timeout_seconds', title: 'settings.downloadApiTimeout', hint: 'settings.downloadApiTimeoutHint' },
  { key: 'first_byte_timeout_seconds', title: 'settings.downloadFirstByteTimeout', hint: 'settings.downloadFirstByteTimeoutHint' },
] as const
type DownloadSetting = typeof rows[number]['key']
const localizedRows = computed(() => rows.map(row => ({
  key: row.key, title: t(row.title), hint: t(row.hint, { value: downloadDefaults[row.key] }),
})))

function currentValue(key: DownloadSetting) {
  return props.config?.network[key] ?? downloadDefaults[key]
}

async function save(key: DownloadSetting, value: number) {
  if (disposed || disabled.value || currentValue(key) === value) return
  if (!Number.isInteger(value) || value < min || value > max) return
  const originalValue = props.config?.network[key]
  saving.value = true
  emit('saving', true)
  try {
    const result = await api.saveSettings({ [key]: value })
    if (disposed) return
    const latest = dashboard.config.value ?? props.config
    if (!latest) return
    // A delayed full response must not undo a newer source or settings snapshot.
    const saved = { ...latest, network: { ...latest.network,
      [key]: latest.network[key] === originalValue ? result.network[key] : latest.network[key],
    } }
    dashboard.applyConfigSnapshot(saved)
    emit('saved', saved)
    if (!demoActive.value) message.success(t('common.saved'))
  } catch (reason) {
    if (!disposed && !demoActive.value) message.error(backendMessageOf(reason))
  } finally {
    saving.value = false
    if (!disposed) emit('saving', false)
  }
}

onBeforeUnmount(() => { disposed = true; emit('saving', false) })
</script>

<template>
  <SettingsGroup :title="t('settings.downloadSettings')" data-testid="download-settings">
    <SettingsRow v-for="row in localizedRows" :key="row.key" :title="row.title" :description="row.hint" :description-title="row.hint">
      <template #right>
        <n-select :value="currentValue(row.key)" :options="secondOptions"
          style="width: 304px; max-width: 100%" :loading="saving" :disabled="disabled"
          :aria-label="row.title" :data-testid="`download-${row.key}`"
          @update:value="(value: number) => save(row.key, value)" />
      </template>
    </SettingsRow>
  </SettingsGroup>
</template>
