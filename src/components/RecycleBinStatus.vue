<script setup lang="ts">
import { nextTick, onMounted, shallowRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import { FolderOpenOutline, TrashOutline } from '@vicons/ionicons5'
import { useRecycleBin } from '../composables/useRecycleBin'
import { useRecycleBinActions } from '../composables/useRecycleBinActions'
import { t } from '../i18n'
import RecycleBinDialog from './RecycleBinDialog.vue'
import RecycleBinSummary from './RecycleBinSummary.vue'
import TitleBarStatusPopover from './TitleBarStatusPopover.vue'

const { count, loading, error, overLimit, busy, reload, ensureLoaded } = useRecycleBin()
const { confirmClearAll } = useRecycleBinActions()
const menuOpen = shallowRef(false)
const dialogOpen = shallowRef(false)

onMounted(() => { void ensureLoaded() })

watch(menuOpen, show => { if (show) void reload() })

function openBin() {
  menuOpen.value = false
  dialogOpen.value = true
}

async function clearBin() {
  menuOpen.value = false
  // The titlebar popover sits above dialogs, so close it before confirmation.
  await nextTick()
  confirmClearAll()
}
</script>

<template>
  <TitleBarStatusPopover v-model:open="menuOpen" :tone="overLimit ? 'warning' : undefined"
    :title="overLimit ? t('recycle.overSizeLimit') : t('recycle.recycleBin')"
    :label="t('recycle.recycleBin')" data-testid="recycle-bin-status">
    <template #icon><TrashOutline width="17" height="17" aria-hidden="true" /></template>
    <div class="recycle-panel" :aria-busy="loading">
      <div class="recycle-panel-heading">
        <strong>{{ t('recycle.recycleBin') }}</strong>
        <RecycleBinSummary />
      </div>
      <p v-if="error" class="recycle-panel-error" role="alert">{{ error }}</p>
      <p v-else-if="overLimit" class="recycle-panel-note">{{ t('recycle.overSizeLimit') }}</p>
      <div class="recycle-panel-actions">
        <n-button size="small" secondary :title="t('recycle.openRecycleBin')" @click="openBin">
          <template #icon><FolderOpenOutline width="15" height="15" /></template>
          {{ t('recycle.openRecycleBin') }}
        </n-button>
        <n-button size="small" quaternary type="error" :disabled="busy || loading || !count"
          :title="t('recycle.emptyRecycleBin')" @click="clearBin">
          {{ t('recycle.emptyRecycleBin') }}
        </n-button>
      </div>
    </div>
  </TitleBarStatusPopover>
  <RecycleBinDialog v-model:show="dialogOpen" />
</template>

<style scoped>
.recycle-panel { display: flex; flex-direction: column; gap: 12px; }
.recycle-panel-heading, .recycle-panel-actions { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.recycle-panel-heading strong { font-size: 14px; }
.recycle-panel-note, .recycle-panel-error { margin: 0; font-size: 12px; line-height: 1.5; }
.recycle-panel-note { color: var(--oc2-text-3); }
.recycle-panel-error { color: var(--oc2-danger, #ef4444); overflow-wrap: anywhere; }
</style>
