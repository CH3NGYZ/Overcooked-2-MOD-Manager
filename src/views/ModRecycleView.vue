<script setup lang="ts">
/**
 * MOD 回收站：管理器删除的 MOD 暂存在用户数据根下的 `recycle-bin/`。
 *
 * 两个动作的语义差别决定了确认强度：
 *   · 恢复 —— 把文件搬回原游戏目录并合并安装清单。可逆、不丢数据，
 *     风险低，失败也能重试，所以直接执行，只把结果用 toast 报出来。
 *   · 彻底删除 —— 移除暂存文件与清单快照，**不可撤销**。必须二次确认，
 *     确认框里点名是哪一个 MOD，避免用户对着列表点错行。
 *
 * 组件全部用 Naive 原生：n-list/n-list-item 承担列表、n-thing 承担标题与描述、
 * n-collapse 承担「文件清单」展开、n-popconfirm 承担删除确认、n-empty 承担空态。
 * 本页只写布局与间距，不重造这些交互。
 */
import { computed, onMounted, ref } from 'vue'
import {
  NButton, NCollapse, NCollapseItem, NEmpty, NList, NListItem, NPopconfirm, NSpin, NTag, NThing, useMessage,
} from 'naive-ui'
import { ArrowUndoOutline, RefreshOutline, TrashOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { t, locale } from '../i18n'
import { tagColorFor } from '../theme'
import { useDashboard } from '../composables/useDashboardStore'
import type { ModRecycleEntry } from '../types'

const message = useMessage()
const { game, ensureLoaded } = useDashboard()

const entries = ref<ModRecycleEntry[]>([])
const loading = ref(false)
const error = ref('')
/** 正在执行的动作：只锁定这一行，不冻结整页（其他行仍可操作）。 */
const acting = ref<{ id: string; kind: 'restore' | 'delete' } | null>(null)

/** 游戏运行时不能恢复（文件会被占用），删除回收站内容不受影响。 */
const restoreBlocked = computed(() => !!game.value?.kitchen_running)

onMounted(() => {
  void ensureLoaded()
  void reload()
})

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

/** 回收站里的包名：优先显示安装时的包名，回退到条目名。 */
function entryTitle(entry: ModRecycleEntry) {
  return entry.name
}

function isActing(entry: ModRecycleEntry, kind: 'restore' | 'delete') {
  return acting.value?.id === entry.id && acting.value.kind === kind
}

/** 批量操作互斥：同一时刻只允许一个动作在跑，避免清单快照互相踩。 */
const busy = computed(() => acting.value !== null)

async function restore(entry: ModRecycleEntry) {
  if (busy.value || restoreBlocked.value) return
  acting.value = { id: entry.id, kind: 'restore' }
  try {
    const result = await api.restoreRecycledMod(entry.id)
    if (result.ok) {
      message.success(result.message || t('恢复'))
      // 恢复后该条目已离开回收站，直接从列表移除；不整页重读
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
</script>

<template>
  <!-- 页头右侧：刷新列表（只读操作，不受游戏运行状态影响） -->
  <Teleport to="#page-header-actions" defer>
    <n-button size="small" secondary :loading="loading" @click="reload">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('立即刷新') }}
    </n-button>
  </Teleport>

  <p v-if="restoreBlocked" class="recycle-warning">{{ t('游戏运行中，暂时不能恢复 MOD。') }}</p>
  <p v-if="error" class="recycle-error" role="alert">{{ error }}</p>

  <n-spin :show="loading">
    <n-empty v-if="!loading && !entries.length && !error" :description="t('回收站为空')" class="empty">
      <template #extra>
        <span class="recycle-hint">{{ t('尚未删除任何 MOD。删除的 MOD 会先暂存在这里，可随时恢复。') }}</span>
      </template>
    </n-empty>

    <n-list v-else-if="entries.length" class="recycle-list" hoverable>
      <n-list-item v-for="entry in entries" :key="entry.id">
        <n-thing :title="entryTitle(entry)">
          <template #header-extra>
            <div class="recycle-actions">
              <n-button size="tiny" secondary :loading="isActing(entry, 'restore')"
                :disabled="busy || restoreBlocked" @click="restore(entry)">
                <template #icon><ArrowUndoOutline width="14" height="14" /></template>
                {{ t('恢复') }}
              </n-button>
              <!--
                彻底删除必须二次确认：清理的是唯一副本（暂存文件 + 清单快照），
                误点无法挽回。确认框里带上名字，避免对着列表点错行。
              -->
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
                  <span class="recycle-confirm-name">{{ entryTitle(entry) }}</span>
                  <span class="recycle-confirm-note">{{ t('将永久删除回收站中的 MOD 文件和保存的安装清单，此操作无法撤销。') }}</span>
                </div>
              </n-popconfirm>
            </div>
          </template>

          <!-- 元信息：标签跟版本，其余用紧凑定义列表 -->
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
/* 空态与其它页面一致：吃掉内容区并在其中居中（高度链见 global.css） */
.empty { flex: 1 1 auto; min-height: 0; margin: 0; }
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
 * 原游戏路径：等宽字体便于逐段核对，超长时单行省略而不是换行撑高卡片。
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
  /* 文件很多时限制高度，避免单个条目把整页撑长 */
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
