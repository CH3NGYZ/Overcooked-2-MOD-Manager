<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NCard, NInput, NSpin, NTag } from 'naive-ui'
import { FolderOpen, RefreshCw, RotateCcw, Save } from 'lucide-vue-next'
import { api } from '../services/tauri'
import { useDashboard } from '../composables/useDashboardStore'

const message = useMessage()
const { load, loading, game } = useDashboard()
const pathDraft = ref('')
const saving = ref(false)
const detecting = ref(false)

onMounted(() => { void load() })

const currentPath = computed(() => game.value?.path ?? '')
const runtimeLabel = computed(() => game.value?.wine_prefix ? 'Wine/Proton' : '本机')

function syncDraft() {
  pathDraft.value = currentPath.value
}
watch(currentPath, syncDraft, { immediate: true })

function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
}

async function detect() {
  detecting.value = true
  try {
    const info = await api.detectGame()
    message.success(info.path ? `检测到游戏：${info.path}` : '未找到游戏目录，请手动设置')
    await load()
  } catch (reason) { fail(reason) } finally { detecting.value = false }
}

async function openDir() {
  const result = await api.openGameDirectory()
  if (!result.ok) message.error(result.message)
}

async function pick() {
  const dir = await api.chooseGameDirectory()
  if (dir) pathDraft.value = dir
}

async function save() {
  const next = pathDraft.value.trim()
  if (!next) { message.warning('请先填写游戏目录'); return }
  saving.value = true
  try {
    const info = await api.setGamePath(next)
    message.success(info.game_exists ? '游戏路径已保存并验证' : '路径已保存，但目录不存在或缺少游戏文件')
    await load()
  } catch (reason) { fail(reason) } finally { saving.value = false }
}

async function reset() {
  saving.value = true
  try {
    const info = await api.resetGamePath()
    message.success(info.path ? `已恢复默认：${info.path}` : '已清除游戏路径')
    await load()
  } catch (reason) { fail(reason) } finally { saving.value = false }
}
</script>

<template>
  <n-spin :show="loading">
    <n-card title="游戏目录" class="block">
      <template #header-extra>
        <n-tag :type="game?.game_exists ? 'success' : 'warning'" size="small">
          {{ game?.game_exists ? '已验证' : '未验证' }}
        </n-tag>
      </template>
      <div class="row">
        <n-input v-model:value="pathDraft" placeholder="选择或输入游戏根目录（包含 Overcooked!2.exe）" clearable />
        <n-button secondary @click="pick"><FolderOpen :size="16" />浏览…</n-button>
      </div>
      <div class="hint muted">
        当前：{{ currentPath || '未设置' }} · 运行形态：{{ runtimeLabel }}
        <template v-if="game?.steam_library"> · Steam 库：{{ game.steam_library }}</template>
        <template v-if="game?.kitchen_running"> · 游戏运行中</template>
      </div>
      <div class="actions">
        <n-button type="primary" :loading="saving" @click="save"><Save :size="16" />保存并验证</n-button>
        <n-button secondary :loading="detecting" @click="detect"><RefreshCw :size="16" />重新检测</n-button>
        <n-button secondary @click="reset"><RotateCcw :size="16" />恢复默认</n-button>
        <n-button quaternary @click="openDir"><FolderOpen :size="16" />打开目录</n-button>
      </div>
    </n-card>
  </n-spin>
</template>

<style scoped>
.block { max-width: 860px; }
.row { display: flex; gap: 10px; }
.row .n-input { flex: 1; }
.hint { margin: 10px 0 16px; }
.actions { display: flex; gap: 10px; flex-wrap: wrap; }
</style>
