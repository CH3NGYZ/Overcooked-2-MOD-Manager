<script setup lang="ts">
/** Entries and mutations share the same process state as the titlebar menu. */
import { computed, onMounted } from 'vue'
import {
  NButton, NCollapse, NCollapseItem, NEmpty, NList, NListItem, NPopconfirm, NSpin, NTag, NThing,
} from 'naive-ui'
import { ArrowUndoOutline, TrashOutline } from '@vicons/ionicons5'
import { pickI18n, t, locale } from '../i18n'
import { tagColorFor } from '../theme'
import { useDashboard } from '../composables/useDashboardStore'
import { useRecycleBin } from '../composables/useRecycleBin'
import { useRecycleBinActions } from '../composables/useRecycleBinActions'
import { formatFileSize } from '../repositoryAssets'
import type { ModRecycleEntry } from '../types'

const { game } = useDashboard()
const { entries, loading, error, loaded, acting, busy, ensureLoaded } = useRecycleBin()
const { restore, remove } = useRecycleBinActions()
const restoreBlocked = computed(() => !!game.value?.kitchen_running)
const isEmpty = computed(() => loaded.value && !loading.value && !entries.value.length && !error.value)

onMounted(() => { void ensureLoaded() })

function entryName(entry: ModRecycleEntry) {
  return pickI18n(entry.name_i18n, entry.name)
}

function deletedTime(value: string) {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString(locale.value)
}

function isActing(entry: ModRecycleEntry, kind: 'restore' | 'delete') {
  return acting.value?.id === entry.id && acting.value.kind === kind
}
</script>

<template>
  <p v-if="restoreBlocked" class="recycle-warning">{{ t('recycle.theGameIsRunningModsCannotBeRestoredRightNow') }}</p>
  <p v-if="error" class="recycle-error" role="alert">{{ error }}</p>

  <n-spin :show="loading">
    <n-empty v-if="isEmpty" :description="t('recycle.recycleBinIsEmpty')" class="recycle-empty">
      <template #extra>
        <span class="recycle-hint">{{ t('recycle.noModsDeletedYetDeletedModsAreKeptHereUntilYouRemoveThemPermanently') }}</span>
      </template>
    </n-empty>

    <n-list v-else-if="entries.length" class="recycle-list" hoverable>
      <n-list-item v-for="entry in entries" :key="entry.id">
        <n-thing :title="entryName(entry)">
          <template #header-extra>
            <div class="recycle-actions">
              <n-button size="tiny" secondary :loading="isActing(entry, 'restore')"
                :disabled="busy" :title="t('overlays.restore')" @click="restore(entry)">
                <template #icon><ArrowUndoOutline width="14" height="14" /></template>
                {{ t('overlays.restore') }}
              </n-button>
              <n-popconfirm :disabled="busy" @positive-click="remove(entry)">
                <template #trigger>
                  <n-button size="tiny" quaternary type="error" :loading="isActing(entry, 'delete')"
                    :disabled="busy" :title="t('recycle.deletePermanently')">
                    <template #icon><TrashOutline width="14" height="14" /></template>
                    {{ t('recycle.deletePermanently') }}
                  </n-button>
                </template>
                <div class="recycle-confirm">
                  <b>{{ t('recycle.deleteModPermanently') }}</b>
                  <span class="recycle-confirm-name">{{ entryName(entry) }}</span>
                  <span class="recycle-confirm-note">{{ t('recycle.thisPermanentlyDeletesTheModFilesAndTheSavedInstallManifestItCannotBeUndone') }}</span>
                </div>
              </n-popconfirm>
            </div>
          </template>

          <template #description>
            <div class="recycle-meta">
              <n-tag v-if="entry.release_tag" size="tiny" :bordered="false" class="state-tag"
                :color="tagColorFor('muted')">{{ entry.release_tag }}</n-tag>
              <span>{{ t('recycle.deleted') }} · {{ deletedTime(entry.deleted_at) }}</span>
              <span>{{ t('recycle.files') }} · {{ entry.file_count }}</span>
              <span :title="t('recycle.recordedFileSize')">{{ entry.size_bytes ? formatFileSize(entry.size_bytes) : '0 B' }}</span>
            </div>
            <div class="recycle-path" :title="entry.game_path">{{ entry.game_path }}</div>
          </template>

          <!-- 文件清单：用 n-collapse 承担展开，不写自绘 disclosure -->
          <n-collapse v-if="entry.files.length" class="recycle-files">
            <n-collapse-item :title="t('recycle.viewFiles')" :name="entry.id">
              <ul class="recycle-file-list">
                <li v-for="file in entry.files" :key="file">{{ file }}</li>
              </ul>
            </n-collapse-item>
          </n-collapse>
        </n-thing>
      </n-list-item>
    </n-list>
  </n-spin>
</template>

<style scoped>
.recycle-warning {
  margin: 0 0 12px;
  padding: 9px 12px;
  border-radius: 9px;
  font-size: 12.5px;
  color: var(--oc2-text-2);
  background: color-mix(in srgb, var(--oc2-tone-moderate-bg, #f59e0b) 12%, transparent);
  border: 1px solid color-mix(in srgb, var(--oc2-tone-moderate-bg, #f59e0b) 32%, transparent);
}
.recycle-error {
  margin: 0 0 12px;
  padding: 9px 12px;
  border-radius: 9px;
  font-size: 12.5px;
  color: #f87171;
  background: color-mix(in srgb, #f87171 10%, transparent);
  border: 1px solid color-mix(in srgb, #f87171 28%, transparent);
}
/* 空态：撑满可用高度并居中（高度链由宿主提供，见 global.css） */
.recycle-empty { flex: 1 1 auto; min-height: 0; margin: 0; }
.recycle-hint {
  display: block;
  max-width: 460px;
  margin-top: 6px;
  font-size: 12.5px;
  line-height: 1.6;
  color: var(--oc2-text-3);
  text-align: center;
}
.recycle-list {
  border-radius: 12px;
  overflow: hidden;
  background: var(--oc2-card);
  border: 1px solid var(--oc2-row-line);
}
.recycle-actions {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}
.recycle-meta {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  font-size: 12.5px;
  color: var(--oc2-text-3);
}
/*
 * 原游戏路径：等宽字体便于逐段核对，超长时单行省略而不是换行撑高条目。
 * title 属性保留完整路径，悬停可看。
 */
.recycle-path {
  margin-top: 6px;
  font-family: Consolas, 'Courier New', monospace;
  font-size: 12px;
  color: var(--oc2-text-4);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.recycle-files { margin-top: 10px; }
.recycle-file-list {
  margin: 0;
  padding: 0 0 0 2px;
  list-style: none;
  font-family: Consolas, 'Courier New', monospace;
  font-size: 12px;
  line-height: 1.85;
  color: var(--oc2-text-3);
  /* 文件很多时限制高度，避免单个条目把整个弹窗撑长 */
  max-height: 220px;
  overflow-y: auto;
}
.recycle-confirm {
  display: flex;
  flex-direction: column;
  gap: 6px;
  max-width: 280px;
}
.recycle-confirm-name {
  font-size: 12.5px;
  color: var(--oc2-text-2);
  word-break: break-all;
}
.recycle-confirm-note {
  font-size: 12px;
  line-height: 1.6;
  color: var(--oc2-text-3);
}
</style>
