<script setup lang="ts">
/**
 * 回收站内容列表（页面与工具页弹窗共用）。
 *
 * 只负责「列出条目 + 恢复 + 彻底删除」，不含页面外壳或弹窗外壳：
 *   · `views/ModRecycleView.vue` 把它放在整页里；
 *   · `components/RecycleBinDialog.vue` 把它放进 AppModal。
 * 抽出来的目的是让两处的行为、文案、二次确认完全一致 —— 各写一份必然分叉。
 *
 * 两个动作的确认强度不同，这是刻意的：
 *   · 恢复可逆、失败可重试 → 直接执行 + toast；
 *   · 彻底删除移除唯一副本（暂存文件 + 清单快照）→ n-popconfirm 二次确认，
 *     并在问句里点名是哪一个 MOD（列表长时容易点错行）。
 */
import { computed, onMounted, ref } from 'vue'
import {
  NButton, NCollapse, NCollapseItem, NEmpty, NList, NListItem, NPopconfirm, NSpin, NTag, NThing, useMessage,
} from 'naive-ui'
import { ArrowUndoOutline, TrashOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { t, locale } from '../i18n'
import { tagColorFor } from '../theme'
import { useDashboard } from '../composables/useDashboardStore'
import type { ModRecycleEntry } from '../types'

const message = useMessage()
const { game } = useDashboard()

const entries = ref<ModRecycleEntry[]>([])
const loading = ref(false)
const error = ref('')
/** 正在执行的动作：只锁这一行，不冻结整个列表（其他行仍可操作）。 */
const acting = ref<{ id: string; kind: 'restore' | 'delete' } | null>(null)

/** 游戏运行时不能恢复（文件被占用）；删除回收站内容不受影响。 */
const restoreBlocked = computed(() => !!game.value?.kitchen_running)
const busy = computed(() => acting.value !== null)
/** 供宿主决定是否显示「刷新」入口等；与列表同一份状态。 */
const isEmpty = computed(() => !loading.value && !entries.value.length && !error.value)

onMounted(() => { void reload() })

async function reload() {
  if (loading.value) return
  loading.value = true
  error.value = ''
  try {
    entries.value = await api.getModRecycleBin()
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    loading.value = false
  }
}

/** 删除时间：按当前界面语言格式化；解析失败时原样显示，不编造时间。 */
function deletedTime(value: string) {
  const date = new Date(value)
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString(locale.value)
}

function isActing(entry: ModRecycleEntry, kind: 'restore' | 'delete') {
  return acting.value?.id === entry.id && acting.value.kind === kind
}

async function restore(entry: ModRecycleEntry) {
  if (busy.value || restoreBlocked.value) return
  acting.value = { id: entry.id, kind: 'restore' }
  try {
    const result = await api.restoreRecycledMod(entry.id)
    if (result.ok) {
      message.success(result.message || t('恢复'))
      entries.value = entries.value.filter((item) => item.id !== entry.id)
    } else {
      message.error(result.message)
    }
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    acting.value = null
  }
}

async function remove(entry: ModRecycleEntry) {
  if (busy.value) return
  acting.value = { id: entry.id, kind: 'delete' }
  try {
    const result = await api.deleteRecycledMod(entry.id)
    if (result.ok) {
      message.success(result.message || t('彻底删除'))
      entries.value = entries.value.filter((item) => item.id !== entry.id)
    } else {
      message.error(result.message)
    }
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    acting.value = null
  }
}

defineExpose({ reload })
</script>

<template>
  <p v-if="restoreBlocked" class="recycle-warning">{{ t('游戏运行中，暂时不能恢复 MOD。') }}</p>
  <p v-if="error" class="recycle-error" role="alert">{{ error }}</p>

  <n-spin :show="loading">
    <n-empty v-if="isEmpty" :description="t('回收站为空')" class="recycle-empty">
      <template #extra>
        <span class="recycle-hint">{{ t('尚未删除任何 MOD。删除的 MOD 会先暂存在这里，可随时恢复。') }}</span>
      </template>
    </n-empty>

    <n-list v-else-if="entries.length" class="recycle-list" hoverable>
      <n-list-item v-for="entry in entries" :key="entry.id">
        <n-thing :title="entry.name">
          <template #header-extra>
            <div class="recycle-actions">
              <n-button size="tiny" secondary :loading="isActing(entry, 'restore')"
                :disabled="busy || restoreBlocked" @click="restore(entry)">
                <template #icon><ArrowUndoOutline width="14" height="14" /></template>
                {{ t('恢复') }}
              </n-button>
              <n-popconfirm :disabled="busy" @positive-click="remove(entry)">
                <template #trigger>
                  <n-button size="tiny" quaternary type="error" :loading="isActing(entry, 'delete')"
                    :disabled="busy" :title="t('彻底删除')">
                    <template #icon><TrashOutline width="14" height="14" /></template>
                    {{ t('彻底删除') }}
                  </n-button>
                </template>
                <div class="recycle-confirm">
                  <b>{{ t('彻底删除 MOD') }}</b>
                  <span class="recycle-confirm-name">{{ entry.name }}</span>
                  <span class="recycle-confirm-note">{{ t('将永久删除回收站中的 MOD 文件和保存的安装清单，此操作无法撤销。') }}</span>
                </div>
              </n-popconfirm>
            </div>
          </template>

          <template #description>
            <div class="recycle-meta">
              <n-tag v-if="entry.release_tag" size="tiny" :bordered="false" class="state-tag"
                :color="tagColorFor('muted')">{{ entry.release_tag }}</n-tag>
              <span>{{ t('删除时间') }} · {{ deletedTime(entry.deleted_at) }}</span>
              <span>{{ t('文件数量') }} · {{ entry.file_count }}</span>
            </div>
            <div class="recycle-path" :title="entry.game_path">{{ entry.game_path }}</div>
          </template>

          <!-- 文件清单：用 n-collapse 承担展开，不写自绘 disclosure -->
          <n-collapse v-if="entry.files.length" class="recycle-files">
            <n-collapse-item :title="t('查看文件清单')" :name="entry.id">
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
