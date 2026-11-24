<script setup lang="ts">
import { computed } from 'vue'
import { locale, t } from '../i18n'
import { mapBundleLabel } from '../mapBundleLabel'
import { formatVersion } from '../modStatus'
import type { LevelSetInfo, LevelSetPlugin } from '../types'
import LocalItemCard from './LocalItemCard.vue'

const { set, plugin, nameI18n, expanded } = defineProps<{
  set: LevelSetInfo
  plugin: LevelSetPlugin
  nameI18n?: Record<string, string>
  expanded: boolean
}>()

const tag = computed(() => {
  const sourceName = nameI18n?.[locale.value]?.trim()
  const name = mapBundleLabel({ label: set.name, name_zh: set.name_zh }, locale.value)
  const role = t(plugin.role === 'dependency' ? 'modsLocal.mapBundledDependency' : 'modsLocal.mapBundledPlugin')
  return `${role} · ${sourceName || name || set.dir_name}`
})
const version = computed(() => formatVersion(plugin.version))
const fields = computed(() => [
  { label: t('modsLocal.name'), value: plugin.name },
  { label: 'GUID', value: plugin.guid, mono: true },
  { label: t('modsOnline.versions'), value: version.value, mono: true },
  { label: t('modsLocal.title'), value: plugin.title },
  { label: t('modsLocal.productName'), value: plugin.product_name },
  { label: t('modsLocal.author'), value: plugin.company },
  { label: t('modsLocal.originalFilename'), value: plugin.original_filename, mono: true },
  { label: t('modsLocal.enabled'), value: t(plugin.enabled ? 'modsLocal.enabled2' : 'modsLocal.disabled') },
])
</script>

<template>
  <LocalItemCard class="map-plugin-card" :data-plugin-file="plugin.file_name" :data-plugin-key="plugin.path ?? plugin.file_name"
    :switchable="false" :disabled="!plugin.enabled" :expanded="expanded"
    :title="plugin.name?.trim() || plugin.file_name.replace(/\.dll(?:\.disabled)?$/i, '')"
    :title-tooltip="plugin.file_name" :title-tag="tag" :version="version || undefined">
    <template #detail>
      <dl class="plugin-details">
        <div v-for="field in fields" :key="field.label">
          <dt>{{ field.label }}</dt>
          <dd :class="{ mono: field.mono }">{{ field.value || '—' }}</dd>
        </div>
        <div class="plugin-path">
          <dt>{{ t('levelSet.path') }}</dt>
          <dd class="mono" :title="plugin.path || plugin.file_name">{{ plugin.path || plugin.file_name }}</dd>
        </div>
      </dl>
    </template>
  </LocalItemCard>
</template>

<style scoped>
.map-plugin-card :deep(.card-title-meta) { min-width: 0; flex: 0 1 auto; }
.map-plugin-card :deep(.n-tag) { min-width: 0; }
.map-plugin-card :deep(.n-tag__content) { overflow: hidden; text-overflow: ellipsis; }
.plugin-details { display: grid; grid-template-columns: repeat(2, minmax(0, 1fr)); gap: 6px 18px; margin: 0; }
.plugin-details > div { display: flex; gap: 10px; font-size: 12px; line-height: 1.5; min-width: 0; }
.plugin-details dt { flex: 0 0 auto; min-width: 64px; color: var(--oc2-text-3); }
.plugin-details dd { margin: 0; min-width: 0; color: var(--oc2-text-2); overflow-wrap: anywhere; }
.plugin-path { grid-column: 1 / -1; }
</style>
