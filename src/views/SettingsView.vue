<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NCard, NSwitch, NTag } from 'naive-ui'
import { Download, RefreshCw } from 'lucide-vue-next'
import { getVersion } from '@tauri-apps/api/app'
import PathSettingCard from '../components/PathSettingCard.vue'
import { api } from '../services/tauri'
import { useDashboard } from '../composables/useDashboardStore'
import type { AppUpdateCheckResult, PublicConfig } from '../types'

const message = useMessage()
const { load, game } = useDashboard()

const version = ref('')
const checking = ref(false)
const checkResult = ref<AppUpdateCheckResult | null>(null)
const installing = ref(false)
const config = ref<PublicConfig | null>(null)
const savingPath = ref(false)

onMounted(async () => {
  version.value = await getVersion().catch(() => '')
  config.value = await api.config().catch(() => null)
  await load()
})

/* ===== 路径管理：游戏目录（未来 DIYLevel 自定义地图框架路径也走 PathSettingCard） ===== */
function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
}

async function openGameDir() {
  const result = await api.openGameDirectory()
  if (!result.ok) message.error(result.message)
}

async function saveGamePath(path: string) {
  savingPath.value = true
  try {
    const info = await api.setGamePath(path)
    message.success(info.game_exists ? '游戏路径已保存并验证' : '路径已保存，但目录不存在或缺少游戏文件')
    await load()
  } catch (reason) { fail(reason) } finally { savingPath.value = false }
}

async function resetGamePath() {
  savingPath.value = true
  try {
    const info = await api.resetGamePath()
    message.success(info.path ? `已恢复默认：${info.path}` : '已清除游戏路径')
    await load()
  } catch (reason) { fail(reason) } finally { savingPath.value = false }
}

/* ===== 程序更新 ===== */
async function checkUpdate() {
  checking.value = true
  try {
    checkResult.value = await api.checkAppUpdate('zh-CN')
    if (checkResult.value.update) message.success(`发现新版本 ${checkResult.value.update.version}`)
    else if (checkResult.value.all_failed) message.error('更新检查失败，可稍后重试')
    else message.success('当前已是最新版本')
  } catch (reason) { fail(reason) } finally { checking.value = false }
}

async function installUpdate() {
  installing.value = true
  try {
    const result = await api.installAppUpdate()
    result.ok ? message.success(result.message || '更新安装已开始') : message.error(result.message)
  } catch (reason) { fail(reason) } finally { installing.value = false }
}

/* ===== 偏好 ===== */
async function savePreference(patch: Parameters<typeof api.saveSettings>[0], label: string) {
  try {
    config.value = await api.saveSettings(patch)
    message.success(label)
  } catch (reason) { fail(reason) }
}
</script>

<template>
  <PathSettingCard
    title="游戏目录"
    :path="game?.path ?? ''"
    :verified="game?.game_exists ?? null"
    :saving="savingPath"
    description="包含 Overcooked!2.exe 的目录；DIYLevel 自定义地图框架路径后续也在此统一管理。"
    @open="openGameDir"
    @save="saveGamePath"
    @reset="resetGamePath"
  />

  <n-card title="关于" class="block">
    <div class="about-row">
      <span>OC2 MOD 管理器</span>
      <n-tag size="small" :bordered="false">v{{ version || '…' }}</n-tag>
    </div>
    <div class="about-row muted">
      <span>Tauri 2 · Vue 3 · Naive UI · Rust 后端</span>
    </div>
  </n-card>

  <n-card title="程序更新" class="block">
    <div class="about-row">
      <span class="muted">内置 Tauri Updater，签名校验后自动安装。</span>
      <n-button size="small" secondary :loading="checking" @click="checkUpdate">
        <RefreshCw :size="14" />检查更新
      </n-button>
    </div>
    <div v-if="checkResult?.update" class="update-box">
      <div class="about-row">
        <b>新版本 {{ checkResult.update.version }}</b>
        <n-button size="small" type="primary" :loading="installing" @click="installUpdate">
          <Download :size="14" />安装更新
        </n-button>
      </div>
      <div v-if="checkResult.update.body" class="muted update-body">{{ checkResult.update.body }}</div>
    </div>
    <div v-else-if="checkResult && !checkResult.all_failed" class="muted">已是最新版本。</div>
    <div v-else-if="checkResult?.all_failed" class="muted">
      全部渠道检查失败，可前往发布页手动下载。
      <n-button size="tiny" quaternary type="primary" @click="api.openUpdateRepository()">打开发布页</n-button>
    </div>
  </n-card>

  <n-card title="偏好" class="block">
    <div class="pref-row">
      <div>
        <div>启动时检查程序更新</div>
        <div class="muted">每天首次启动时检查新版本</div>
      </div>
      <n-switch :value="config?.network.check_updates_on_start"
        @update:value="(value: boolean) => savePreference({ check_updates_on_start: value }, '已保存')" />
    </div>
    <div class="pref-row">
      <div>
        <div>自动检测游戏路径</div>
        <div class="muted">启动与刷新时自动探测 Steam 库</div>
      </div>
      <n-switch :value="config?.game.auto_detect"
        @update:value="(value: boolean) => savePreference({ auto_detect: value }, '已保存')" />
    </div>
  </n-card>
</template>

<style scoped>
.block { max-width: 860px; margin-bottom: 18px; }
.about-row { display: flex; align-items: center; justify-content: space-between; gap: 12px; padding: 4px 0; }
.update-box {
  margin-top: 10px; padding: 12px; border: 1px solid #2c3540; border-radius: 8px;
  background: #1a1d22;
}
.update-body { margin-top: 8px; white-space: pre-wrap; max-height: 160px; overflow: auto; }
.pref-row {
  display: flex; align-items: center; justify-content: space-between; gap: 16px;
  padding: 9px 0; border-bottom: 1px dashed #232a33;
}
.pref-row:last-child { border-bottom: 0; }
</style>
