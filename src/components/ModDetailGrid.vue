<script setup lang="ts">
/**
 * 单个 DLL 的详情网格。
 *
 * 抽成组件是因为「单 DLL 包」和「多 DLL 包内的子 DLL」两种卡片
 * 只有外壳不同，详情内容完全一样；共用一份避免两处漂移。
 */
import { pickI18n, t } from '../i18n'
import { formatVersion } from '../modStatus'
import type { InstalledMod } from '../types'

defineProps<{
  mod: InstalledMod
  bindingText: string
  statusSummary: string
  packageCountText: string
}>()

/** 包名快照：按当前界面语言取，没有快照时回退后端给的单语值。 */
function packageNameText(mod: InstalledMod) {
  return pickI18n(mod.package_name_i18n, mod.package_name ?? '').trim()
}
</script>

<template>
  <div class="detail-grid">
    <div class="detail-item"><span class="muted">{{ t('modsLocal.name') }}</span><b>{{ mod.name }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.title') }}</span><b>{{ mod.title || '—' }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.productName') }}</span><b>{{ mod.product_name || '—' }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.originalFilename') }}</span><b class="mono">{{ mod.original_filename || '—' }}</b></div>
    <div class="detail-item"><span class="muted">GUID</span><b class="mono">{{ mod.guid || '—' }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.pluginName') }}</span><b>{{ mod.plugin_name || '—' }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.author') }}</span><b>{{ mod.company || '—' }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.copyright') }}</span><b>{{ mod.copyright || '—' }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsOnline.versions') }}</span><b class="mono">{{ formatVersion(mod.version) || '—' }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.versionSource') }}</span><b>{{ mod.version_source || '—' }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.enabled') }}</span><b>{{ mod.enabled ? t('modsLocal.enabled2') : t('modsLocal.disabled') }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.configFile') }}</span><b>{{ mod.has_config ? t('modsLocal.yes') : t('modsLocal.no') }}</b></div>
    <div class="detail-item"><span class="muted">{{ t('modsLocal.duplicateGuid') }}</span><b>{{ mod.duplicate_guid ? t('modsLocal.yes2') : t('modsLocal.no2') }}</b></div>
    <div class="detail-item wide"><span class="muted">{{ t('modsLocal.file2') }}</span><b class="mono">{{ mod.dll_file }}</b></div>
    <div class="detail-item wide"><span class="muted">{{ t('modsLocal.boundRepository') }}</span><b class="mono">{{ bindingText }}</b></div>
    <div class="detail-item wide"><span class="muted">{{ t('modsLocal.statusLabels') }}</span><b>{{ statusSummary || '—' }}</b></div>
    <div v-if="packageNameText(mod)" class="detail-item wide">
      <span class="muted">{{ t('tools.modPackage') }}</span>
      <b>{{ packageNameText(mod) }}（{{ packageCountText }}）</b>
    </div>
    <div v-if="mod.load_warning" class="detail-item wide">
      <span class="muted">{{ t('modsLocal.loadWarning') }}</span>
      <b class="warn">{{ mod.load_warning }}</b>
    </div>
  </div>
</template>

<style scoped>
.detail-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 16px;
}
.detail-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.detail-item.wide { grid-column: 1 / -1; }
.detail-item span { font-size: 11px; }
.detail-item b {
  font-size: 12px;
  color: var(--oc2-text-1);
  font-weight: 500;
  overflow-wrap: anywhere;
}
.detail-item b.warn { color: var(--warning, #e6b86a); }
</style>
