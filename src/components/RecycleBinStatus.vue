<script setup lang="ts">
import { nextTick, onMounted, shallowRef } from 'vue'
import { NButton, NPopover } from 'naive-ui'
import { FolderOpenOutline, TrashOutline } from '@vicons/ionicons5'
import { useRecycleBin } from '../composables/useRecycleBin'
import { useRecycleBinActions } from '../composables/useRecycleBinActions'
import { t } from '../i18n'
import RecycleBinDialog from './RecycleBinDialog.vue'
import RecycleBinSummary from './RecycleBinSummary.vue'

const { count, loading, error, overLimit, busy, reload, ensureLoaded } = useRecycleBin()
const { confirmClearAll } = useRecycleBinActions()
const menuOpen = shallowRef(false)
const dialogOpen = shallowRef(false)

onMounted(() => { void ensureLoaded() })

function showMenu(show: boolean) {
  menuOpen.value = show
  if (show) void reload()
}

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
  <n-popover :show="menuOpen" trigger="click" placement="bottom-end" :z-index="5000" :show-arrow="false"
    @update:show="showMenu">
    <template #trigger>
      <button class="recycle-button" :class="{ 'over-limit': overLimit }" type="button"
        :title="overLimit ? t('recycle.overSizeLimit') : t('recycle.recycleBin')"
        :aria-label="t('recycle.recycleBin')" :aria-expanded="menuOpen" aria-haspopup="dialog"
        data-testid="recycle-bin-status">
        <TrashOutline width="17" height="17" />
        <span v-if="overLimit" class="size-dot" aria-hidden="true" />
      </button>
    </template>
    <section class="recycle-panel" role="dialog" :aria-label="t('recycle.recycleBin')" :aria-busy="loading">
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
    </section>
  </n-popover>
  <RecycleBinDialog v-model:show="dialogOpen" />
</template>

<style scoped>
.recycle-button { position: relative; display: grid; place-items: center; width: 32px; height: 32px; padding: 0; border-radius: 9px; border: 1px solid var(--oc2-rail-line); background: var(--oc2-card); color: var(--oc2-text-3); cursor: pointer; transition: color .16s, background-color .16s, border-color .16s; }
.recycle-button:hover, .recycle-button:focus-visible, .recycle-button.over-limit { color: var(--oc2-accent); }
.recycle-button.over-limit { border-color: var(--oc2-accent); background: color-mix(in srgb, var(--oc2-accent) 15%, var(--oc2-card)); }
.recycle-button:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
.size-dot { position: absolute; right: 4px; bottom: 4px; width: 4px; height: 4px; border-radius: 50%; background: currentColor; }
.recycle-panel { display: flex; flex-direction: column; gap: 12px; width: 272px; max-width: calc(100vw - 40px); color: var(--oc2-text-2); }
.recycle-panel-heading, .recycle-panel-actions { display: flex; align-items: center; justify-content: space-between; gap: 12px; }
.recycle-panel-heading strong { font-size: 14px; }
.recycle-panel-note, .recycle-panel-error { margin: 0; font-size: 12px; line-height: 1.5; }
.recycle-panel-note { color: var(--oc2-text-3); }
.recycle-panel-error { color: var(--oc2-danger, #ef4444); overflow-wrap: anywhere; }
@media (prefers-reduced-motion: reduce) { .recycle-button { transition: none; } }
</style>
