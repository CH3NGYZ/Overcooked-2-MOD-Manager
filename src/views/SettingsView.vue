<script setup lang="ts">
import { t } from '../i18n'
import { onMounted, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NDrawer, NDrawerContent, NSwitch, NTag } from 'naive-ui'
import { ArrowUndoOutline, DownloadOutline, FolderOpenOutline, InformationCircleOutline } from '@vicons/ionicons5'
import { getVersion } from '@tauri-apps/api/app'
import PathEditor from '../components/PathEditor.vue'
import UpdatePrompt from '../components/UpdatePrompt.vue'
import SettingsGroup from '../components/settings/SettingsGroup.vue'
import SettingsRow from '../components/settings/SettingsRow.vue'
import { api } from '../services/tauri'
import { useDashboard } from '../composables/useDashboardStore'
import type { AppUpdateCheckResult, PublicConfig } from '../types'

const message = useMessage()
const { load, game } = useDashboard()

const version = ref('')
const checking = ref(false)
const config = ref<PublicConfig | null>(null)
const savingPath = ref(false)

const pathDrawer = ref(false)
const updateDrawer = ref(false)
const updateResult = ref<AppUpdateCheckResult | null>(null)
const updateInfo = ref<NonNullable<AppUpdateCheckResult['update']> | null>(null)
const showUpdate = ref(false)

onMounted(async () => {
  version.value = await getVersion().catch(() => '')
  config.value = await api.config().catch(() => null)
  await load()
})

function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
}

/* ===== 路径管理：游戏目录 ===== */
async function openGameDir() {
  const result = await api.openGameDirectory()
  if (!result.ok) message.error(result.message)
}

async function resetGamePath() {
  savingPath.value = true
  try {
    const info = await api.resetGamePath()
    message.success(info.path || t('恢复默认'))
    await load()
  } catch (reason) { fail(reason) } finally { savingPath.value = false }
}

async function saveGamePath(path: string) {
  savingPath.value = true
  try {
    const info = await api.setGamePath(path)
    message.success(t(info.game_exists ? '已验证' : '这个目录不存在或缺少游戏文件'))
    pathDrawer.value = false
    await load()
  } catch (reason) { fail(reason) } finally { savingPath.value = false }
}

/* ===== 程序更新 ===== */
async function checkUpdate() {
  checking.value = true
  try {
    updateResult.value = await api.checkAppUpdate('zh-CN')
    if (updateResult.value.update) {
      // 检测到更新：与启动时相同的弹窗，更新/取消。
      updateInfo.value = updateResult.value.update
      showUpdate.value = true
    } else if (updateResult.value.all_failed) {
      message.error(t('更新检查失败，可稍后重试'))
    } else {
      message.success(t('当前已是最新版本'))
    }
  } catch (reason) { fail(reason) } finally { checking.value = false }
}

/* ===== 通用偏好 ===== */
async function savePreference(patch: Parameters<typeof api.saveSettings>[0], label: string) {
  try {
    config.value = await api.saveSettings(patch)
    message.success(label)
  } catch (reason) { fail(reason) }
}
</script>

<template>
  <!-- 路径管理 -->
  <SettingsGroup :title="t('路径管理')">
    <SettingsRow :title="t('游戏目录')" :description="t('游戏与 BepInEx 框架所在目录；DIYLevel 自定义地图框架路径后续也在此管理')">
      <template #right>
        <n-button size="small" secondary :disabled="savingPath" @click.stop="resetGamePath">
          <ArrowUndoOutline width="14" height="14" />{{ t('恢复默认路径') }}</n-button>
        <n-button size="small" secondary @click.stop="openGameDir">
          <FolderOpenOutline width="14" height="14" />{{ t('打开路径') }}</n-button>
        <n-button size="small" type="primary" @click.stop="pathDrawer = true">
          {{ t('选择路径') }}
        </n-button>
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!-- 程序更新 -->
  <SettingsGroup :title="t('程序更新')">
    <SettingsRow :title="t('立即检测更新')" :description="t('检查是否有新版本可用，检测到更新会弹出确认')">
      <template #right>
        <n-button size="small" secondary :loading="checking" @click.stop="checkUpdate">
          <DownloadOutline width="14" height="14" />{{ t('检测更新') }}</n-button>
      </template>
    </SettingsRow>
    <SettingsRow :title="t('更新设置')" :description="t('自动检查与安装偏好')" clickable @click="updateDrawer = true" />
  </SettingsGroup>

  <!-- 通用设置 -->
  <SettingsGroup :title="t('通用设置')">
    <SettingsRow :title="t('启动时检查程序更新')" :description="t('每天首次启动时检查新版本')">
      <template #right>
        <n-switch size="small" :value="config?.network.check_updates_on_start"
          @update:value="(value: boolean) => savePreference({ check_updates_on_start: value }, t('已保存'))" />
      </template>
    </SettingsRow>
    <SettingsRow :title="t('自动检测游戏路径')" :description="t('启动与刷新时自动探测 Steam 库')">
      <template #right>
        <n-switch size="small" :value="config?.game.auto_detect"
          @update:value="(value: boolean) => savePreference({ auto_detect: value }, t('已保存'))" />
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!-- 关于 -->
  <SettingsGroup :title="t('关于')">
    <SettingsRow :icon="InformationCircleOutline" :title="t('OC2 MOD 管理器')" :description="t('关于') + ' · Tauri 2 / Vue 3 / Naive UI / Rust'">
      <template #right>
        <n-tag size="small" :bordered="false">v{{ version || '…' }}</n-tag>
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!-- 二级：选择路径 -->
  <n-drawer v-model:show="pathDrawer" :width="440" placement="right">
    <n-drawer-content :title="t('选择游戏目录')" closable>
      <PathEditor :path="game?.path ?? ''" :saving="savingPath" @save="saveGamePath" />
      <div class="muted drawer-hint">{{ t('目录需包含 Overcooked!2.exe；保存后自动验证。') }}</div>
    </n-drawer-content>
  </n-drawer>

  <!-- 二级：更新设置 -->
  <n-drawer v-model:show="updateDrawer" :width="440" placement="right">
    <n-drawer-content :title="t('更新设置')" closable>
      <div class="drawer-pref">
        <div>
          <b>{{ t('启动时检查程序更新') }}</b>
          <div class="muted">{{ t('每天首次启动时检查新版本') }}</div>
        </div>
        <n-switch size="small" :value="config?.network.check_updates_on_start"
          @update:value="(value: boolean) => savePreference({ check_updates_on_start: value }, t('已保存'))" />
      </div>
      <div class="muted drawer-hint">
        {{ t('更新检查按地区走 Gitee / GitHub 双渠道；更新包经 minisign 签名校验后由 Tauri Updater 安装。') }}
      </div>
      <div v-if="updateResult?.all_failed" class="muted drawer-hint danger">
        {{ t('全部渠道检查失败，可前往发布页手动下载。') }}
        <n-button size="tiny" quaternary type="primary" @click="api.openUpdateRepository()">{{ t('打开发布页') }}</n-button>
      </div>
    </n-drawer-content>
  </n-drawer>

  <UpdatePrompt v-model:show="showUpdate" :update="updateInfo" />
</template>

<style scoped>
.drawer-hint { margin-top: 14px; }
.drawer-hint.danger { color: #ee8585; }
.drawer-pref {
  display: flex; align-items: center; justify-content: space-between; gap: 14px;
  padding: 10px 0 14px; border-bottom: 1px dashed var(--oc2-rail-line); margin-bottom: 6px;
}
.drawer-pref b { font-size: 13.5px; }
</style>
