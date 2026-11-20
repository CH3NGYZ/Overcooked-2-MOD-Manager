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

const speedValues = [32, 64, 128, 256, 512, 1024, 2048]
const speedOptions = speedValues.map(value => ({ value, label: `${value} KB/s` }))
const secondOptions = computed(() => Array.from({ length: 16 }, (_, index) => {
  const value = index + 5
  return { value, label: t('settings.downloadSeconds', { seconds: value }) }
}))
const rows = [
  { key: 'race_min_speed_kbps', title: 'settings.downloadMinSpeed', hint: 'settings.downloadMinSpeedHint' },
  { key: 'race_window_seconds', title: 'settings.downloadRaceWindow', hint: 'settings.downloadRaceWindowHint' },
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
  if (!Number.isInteger(value) || (key === 'race_min_speed_kbps' ? !speedValues.includes(value) : value < 5 || value > 20)) return
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
        <n-select :value="currentValue(row.key)" :options="row.key === 'race_min_speed_kbps' ? speedOptions : secondOptions"
          style="width: 304px; max-width: 100%" :loading="saving" :disabled="disabled"
          :aria-label="row.title" :data-testid="`download-${row.key}`"
          @update:value="(value: number) => save(row.key, value)" />
      </template>
    </SettingsRow>
  </SettingsGroup>
</template>
