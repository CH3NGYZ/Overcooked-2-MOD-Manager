<script setup lang="ts">
import { t } from '../i18n'
import { computed, onMounted, ref } from 'vue'
import { useDialog, useMessage } from 'naive-ui'
import { NButton, NCard, NSelect, NSpin, NSwitch, NTag } from 'naive-ui'
import { AddCircleOutline, FolderOpenOutline, SaveOutline, TrashOutline, WarningOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { useDashboard } from '../composables/useDashboardStore'
import type { BepInExInstallResult, OperationResult, Provider } from '../types'

const message = useMessage()
const dialog = useDialog()
const { load, loading, game, config, frameworkOk } = useDashboard()

const working = ref(false)
const installProvider = ref<Provider>('gitee')
const installing = ref(false)

onMounted(() => { void load() })

const winhttpEnabled = computed(() => game.value?.winhttp_state === 'modenabled')
const winhttpLabel = computed(() => {
  switch (game.value?.winhttp_state) {
    case 'modenabled': return '框架已启用'
    case 'moddisabled': return '框架已禁用'
    case 'missing': return '未找到加载器'
    default: return t('状态冲突')
  }
})
const backups = computed(() => game.value?.bepinex.backups ?? [])
const isWindows = computed(() => config.value?.platform === 'windows')

function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
}

async function afterInstall(result: BepInExInstallResult) {
  if (result.ok) message.success(result.message || t('安装框架'))
  else message.error(result.message)
  if (result.wine_override_error) message.warning(result.wine_override_error)
  await load()
}

async function install(reinstall: boolean) {
  installing.value = true
  try {
    await afterInstall(await api.installBepInEx(reinstall, installProvider.value))
  } catch (reason) { fail(reason) } finally { installing.value = false }
}

function uninstall() {
  dialog.warning({
    title: t('卸载 BepInEx 框架'),
    content: t('将移除游戏目录中的 BepInEx 框架文件。建议先创建备份。继续卸载？'),
    positiveText: t('卸载'),
    negativeText: t('取消'),
    async onPositiveClick() {
      working.value = true
      try {
        const result: OperationResult = await api.uninstallBepInEx()
        result.ok ? message.success(result.message || t('卸载')) : message.error(result.message)
        await load()
      } catch (reason) { fail(reason) } finally { working.value = false }
    },
  })
}

async function backup() {
  working.value = true
  try {
    const result = await api.backupBepInEx()
    result.ok ? message.success(result.message || t('创建备份')) : message.error(result.message)
    await load()
  } catch (reason) { fail(reason) } finally { working.value = false }
}

async function restore(backup: string) {
  working.value = true
  try {
    const result = await api.restoreBepInEx(backup)
    result.ok ? message.success(result.message || '已恢复') : message.error(result.message)
    await load()
  } catch (reason) { fail(reason) } finally { working.value = false }
}

function removeBackup(backup: string) {
  dialog.warning({
    title: t('删除备份'),
    content: `永久删除该备份？\n${backup}`,
    positiveText: '删除',
    negativeText: t('取消'),
    async onPositiveClick() {
      const result = await api.deleteBepInExBackup(backup)
      result.ok ? message.success(result.message || '备份已删除') : message.error(result.message)
      await load()
    },
  })
}

async function toggleFramework(enabled: boolean) {
  const result = await api.switchWinhttp(enabled)
  result.ok ? message.success(result.message || (enabled ? t('框架已启用') : t('框架已禁用'))) : message.error(result.message)
  await load()
}

async function openBackups() {
  const result = await api.openBackupDirectory()
  if (!result.ok) message.error(result.message)
}
</script>

<template>
  <n-spin :show="loading">
    <n-card :title="t('BepInEx 框架')" class="block">
      <template #header-extra>
        <n-tag :type="frameworkOk ? 'success' : 'warning'" size="small">
          {{ frameworkOk ? (game?.bepinex.complete ? '已安装' : t('不完整')) : t('未安装') }}
        </n-tag>
      </template>

      <div class="status-row">
        <span class="muted">版本：{{ game?.bepinex.version || '—' }}</span>
        <span class="muted">加载器：{{ winhttpLabel }}</span>
        <span v-if="!isWindows" class="muted">运行形态：{{ config?.game.runtime === 'native' ? '原生' : 'Wine' }}</span>
      </div>

      <div class="field">
        <span class="muted">{{ t('下载渠道') }}</span>
        <n-select v-model:value="installProvider" style="width: 200px" :options="[
          { label: 'Gitee（中国大陆推荐）', value: 'gitee' },
          { label: 'GitHub', value: 'github' },
        ]" />
      </div>

      <div class="actions">
        <n-button type="primary" :loading="installing" :disabled="working"
          @click="install(!!game?.bepinex.installed)">
          <AddCircleOutline width="16" height="16" />{{ game?.bepinex.installed ? t('重新安装') : t('安装框架') }}
        </n-button>
        <n-button secondary :disabled="working || !game?.bepinex.installed" @click="backup">
          <SaveOutline width="16" height="16" />{{ t('创建备份') }}</n-button>
        <n-button secondary type="warning" :disabled="working || !game?.bepinex.installed" @click="uninstall">
          <WarningOutline width="16" height="16" />{{ t('卸载框架') }}</n-button>
        <n-button quaternary :disabled="working" @click="openBackups">
          <FolderOpenOutline width="16" height="16" />{{ t('打开备份目录') }}</n-button>
      </div>

      <div class="switch-row">
        <span>{{ t('框架启用（游戏加载 MOD）') }}</span>
        <n-switch :value="winhttpEnabled" :disabled="working || !game?.bepinex.installed"
          @update:value="toggleFramework" />
      </div>
    </n-card>

    <n-card :title="t('框架备份')" class="block">
      <div v-if="!backups.length" class="muted">{{ t('暂无备份。卸载或重装前建议先创建备份。') }}</div>
      <div v-for="backup in backups" :key="backup" class="backup-row">
        <span class="mono backup-path">{{ backup }}</span>
        <span class="backup-acts">
          <n-button size="small" secondary @click="restore(backup)">{{ t('恢复') }}</n-button>
          <n-button size="small" quaternary type="error" @click="removeBackup(backup)">
            <TrashOutline width="14" height="14" />
          </n-button>
        </span>
      </div>
    </n-card>
  </n-spin>
</template>

<style scoped>
.block { max-width: 980px; margin-bottom: 18px; }
.status-row { display: flex; gap: 22px; flex-wrap: wrap; margin-bottom: 14px; }
.field { display: flex; align-items: center; gap: 12px; margin-bottom: 16px; }
.actions { display: flex; gap: 10px; flex-wrap: wrap; margin-bottom: 14px; }
.switch-row {
  display: flex; align-items: center; justify-content: space-between;
  max-width: 420px; padding-top: 12px; border-top: 1px solid var(--oc2-divider);
}
.backup-row {
  display: flex; align-items: center; justify-content: space-between; gap: 12px;
  padding: 8px 0; border-bottom: 1px dashed var(--oc2-rail-line);
}
.backup-row:last-child { border-bottom: 0; }
.backup-path { flex: 1; min-width: 0; }
.backup-acts { display: flex; gap: 6px; flex: 0 0 auto; }
</style>
