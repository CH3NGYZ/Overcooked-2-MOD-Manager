<script setup lang="ts">
/**
 * BepInEx 框架管理面板（放在设置页右侧抽屉里）。
 *
 * 迁自原「框架管理」独立页面。用户判断：框架管理和装一个 MOD 差不多复杂，
 * 不值得占一个常驻导航位，所以收进设置——卡片只展示版本与启用状态，
 * 所有操作在抽屉里完成。
 *
 * 四个主操作对应四种「路径」问题：
 *   · 恢复默认路径 —— 探测结果不对时，把游戏目录交回自动探测
 *   · 打开路径     —— 去文件管理器里看/改
 *   · 选择路径     —— 手动指定
 *   · 配置管理     —— 编辑 `BepInEx/config/BepInEx.cfg`（复用 ModConfigEditor）
 * 安装/重装/卸载/备份/恢复仍在下方，属于框架本身的维护。
 */
import { computed, ref } from 'vue'
import { useDialog, useMessage } from 'naive-ui'
import { NButton, NSelect, NSpin, NSwitch, NTag } from 'naive-ui'
import {
  AddCircleOutline, FolderOpenOutline, SaveOutline, SettingsOutline, TrashOutline, WarningOutline,
  RefreshOutline,
} from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { t } from '../i18n'
import { tagColorFor } from '../theme'
import { useDashboard } from '../composables/useDashboardStore'
import ModConfigEditor from './ModConfigEditor.vue'
import BepInExSetupGuide from './BepInExSetupGuide.vue'
import BusyOverlay from './BusyOverlay.vue'
import type { BepInExInstallResult, GameRuntime, OperationResult, Provider } from '../types'

const message = useMessage()
const dialog = useDialog()
const { load, loading, game, config, frameworkOk } = useDashboard()

const working = ref(false)
const installProvider = ref<Provider>('gitee')
const installing = ref(false)
/** BepInEx 配置编辑器（与 MOD 配置共用同一个组件，数据源切到 bepinex）。 */
const configEditorOpen = ref(false)

const winhttpEnabled = computed(() => game.value?.winhttp_state === 'modenabled')
const backups = computed(() => game.value?.bepinex.backups ?? [])
const isWindows = computed(() => config.value?.platform === 'windows')
const installed = computed(() => !!game.value?.bepinex.installed)

function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
}

async function afterInstall(result: BepInExInstallResult) {
  if (result.ok) message.success(result.message || t('安装框架'))
  else message.error(result.message)
  if (result.wine_override_error) message.warning(result.wine_override_error)
  await load()
}

/* ===== 平台引导（仅非 Windows） ===== */

/**
 * 引导弹窗的阶段：
 *   · `before-install` —— CrossOver：容器与字体要先准备好，装完才起得来
 *   · `after-install`  —— 原生 macOS：装完再给 Steam 启动项命令
 * `null` 表示不显示。
 */
const guidePhase = ref<'before-install' | 'after-install' | null>(null)
/** 引导确认后要继续执行的安装动作（CrossOver 走这条）。 */
const pendingInstall = ref<{ reinstall: boolean } | null>(null)

const platform = computed(() => config.value?.platform ?? 'windows')
const runtime = computed<GameRuntime>(() => config.value?.game.runtime ?? 'wine')
/** 只有非 Windows 才需要引导：Windows 装完即可用。 */
const needsGuide = computed(() => platform.value !== 'windows')
/** CrossOver / Wine 分支（macOS 与 Linux 的非原生）。 */
const isWineRuntime = computed(() => runtime.value !== 'native')

/** 真正执行安装（引导之外的部分）。 */
async function runInstall(reinstall: boolean) {
  installing.value = true
  try {
    const result = await api.installBepInEx(reinstall, installProvider.value)
    await afterInstall(result)
    /*
     * 原生 macOS：安装成功后先修 Steam Overlay 注入，再弹启动命令引导。
     * 顺序不能反 —— Overlay 修复是自动的，不该插在用户的手动步骤中间；
     * 而且修复失败也不该阻止用户拿到启动命令（BepInEx 仍能加载）。
     */
    if (result.ok && platform.value === 'macos' && runtime.value === 'native') {
      try {
        const repaired = await api.repairNativeBepInExOverlay()
        if (!repaired.ok) message.warning(repaired.message)
      } catch (reason) {
        // 修复失败只提示，不阻断后续引导
        message.warning(reason instanceof Error ? reason.message : String(reason))
      }
      guidePhase.value = 'after-install'
    }
  } catch (reason) {
    fail(reason)
  } finally {
    installing.value = false
  }
}

/**
 * 安装入口：按平台决定是否先弹引导。
 *
 * 平台判断必须读**当前**的 config。这里踩过一次：早期用 computed 提前算好
 * `needsGuide`，而抽屉可能在 Dashboard 还没加载完时就被打开，`config` 为 null
 * 会退化成 `platform = 'windows'`，于是 CrossOver 分支被当成 Windows
 * 直接安装、跳过了容器准备引导。
 * 现在点击时先确保数据就绪，再判断。
 */
async function install(reinstall: boolean) {
  if (!config.value) await load()
  if (needsGuide.value && isWineRuntime.value) {
    // CrossOver：先让用户准备容器，确认后再真正安装
    pendingInstall.value = { reinstall }
    guidePhase.value = 'before-install'
    return
  }
  void runInstall(reinstall)
}

/** 引导确认：CrossOver 继续安装；原生只是关闭（安装已完成）。 */
function onGuideConfirmed() {
  const pending = pendingInstall.value
  pendingInstall.value = null
  if (pending) void runInstall(pending.reinstall)
}

function uninstall() {
  dialog.warning({
    title: t('卸载 BepInEx 框架'),
    // 卸载会把框架 ZIP 解压出的全部文件移入回收站（含游戏根目录的
    // winhttp.dll / doorstop_config.ini / run_bepinex.sh），可随时恢复。
    content: t('将把游戏目录中的 BepInEx 框架文件移入回收站，可在回收站中恢复。继续卸载？'),
    positiveText: t('卸载'),
    negativeText: t('取消'),
    async onPositiveClick() {
      working.value = true
      // 卸载 = 先整目录备份，再按清单回收，两步都是磁盘操作，要遮罩
      busyText.value = t('正在备份并卸载 BepInEx 框架…')
      try {
        const result: OperationResult = await api.uninstallBepInEx()
        result.ok ? message.success(result.message || t('卸载')) : message.error(result.message)
        await load()
      } catch (reason) { fail(reason) } finally {
        working.value = false
        busyText.value = null
      }
    },
  })
}

/**
 * 全屏遮罩文案：`null` 不显示。
 *
 * 备份与卸载都要整目录拷贝（BepInEx 里可能几万个文件、几百 MB），
 * 期间只靠按钮转圈不够 —— 用户会以为卡死并尝试切页，反而制造一个
 * 「备份还在跑、界面已经换了」的错觉。全屏遮罩明确告诉他「在等」。
 */
const busyText = ref<string | null>(null)

async function backup() {
  working.value = true
  busyText.value = t('正在备份 BepInEx 框架…')
  try {
    const result = await api.backupBepInEx()
    result.ok ? message.success(result.message || t('创建备份')) : message.error(result.message)
    await load()
  } catch (reason) { fail(reason) } finally {
    working.value = false
    busyText.value = null
  }
}

async function restore(path: string) {
  working.value = true
  busyText.value = t('正在恢复 BepInEx 框架…')
  try {
    const result = await api.restoreBepInEx(path)
    result.ok ? message.success(result.message || t('已恢复')) : message.error(result.message)
    await load()
  } catch (reason) { fail(reason) } finally {
    working.value = false
    busyText.value = null
  }
}

function removeBackup(path: string) {
  dialog.warning({
    title: t('删除备份'),
    content: t('永久删除该备份？') + `\n${path}`,
    positiveText: t('删除'),
    negativeText: t('取消'),
    async onPositiveClick() {
      const result = await api.deleteBepInExBackup(path)
      result.ok ? message.success(result.message || t('备份已删除')) : message.error(result.message)
      await load()
    },
  })
}

async function toggleFramework(enabled: boolean) {
  const result = await api.switchWinhttp(enabled)
  result.ok
    ? message.success(result.message || (enabled ? t('框架已启用') : t('框架已禁用')))
    : message.error(result.message)
  await load()
}

/* ===== 路径相关的四个操作 ===== */

/** 打开游戏的 BepInEx 目录（需要在管理器里能看到文件）。 */
async function openPath() {
  const result = await api.openGameDirectory()
  if (!result.ok) message.error(result.message)
}

/** 重新自动探测游戏目录。 */
function restoreDefaultPath() {
  dialog.warning({
    title: t('恢复默认路径'),
    content: t('将清除手动选择的游戏目录，改回自动探测 Steam 库。继续？'),
    positiveText: t('恢复默认'),
    negativeText: t('取消'),
    async onPositiveClick() {
      working.value = true
      try {
        await api.resetGamePath()
        message.success(t('已恢复默认路径'))
        await load()
      } catch (reason) { fail(reason) } finally { working.value = false }
    },
  })
}

/** 手动选择游戏目录。 */
async function choosePath() {
  working.value = true
  try {
    const picked = await api.chooseGameDirectory()
    const path = Array.isArray(picked) ? picked[0] : picked
    if (!path) return
    await api.setGamePath(path)
    message.success(t('已保存'))
    await load()
  } catch (reason) { fail(reason) } finally { working.value = false }
}

async function openBackups() {
  const result = await api.openBackupDirectory()
  if (!result.ok) message.error(result.message)
}
</script>

<template>
  <n-spin :show="loading">
    <!-- 状态摘要：版本 + 启用开关 + 安装状态 -->
    <div class="panel-head">
      <div class="panel-head-main">
        <n-switch v-if="installed" :value="winhttpEnabled" :title="t('框架启用（游戏加载 MOD）')"
          :disabled="working" @update:value="toggleFramework" />
        <span class="panel-title">{{ t('BepInEx 框架') }}</span>
        <span class="panel-version muted">{{ t('版本') }}：{{ game?.bepinex.version || '—' }}</span>
        <n-tag size="small" :bordered="false" class="state-tag" :color="tagColorFor(frameworkOk ? 'accent' : 'moderate')">
          {{ frameworkOk ? (game?.bepinex.complete ? t('已安装') : t('不完整')) : t('未安装') }}
        </n-tag>
      </div>
      <span v-if="!isWindows" class="panel-runtime muted">
        {{ t('运行形态') }}：{{ config?.game.runtime === 'native' ? t('原生') : 'Wine' }}
      </span>
    </div>

    <!--
      四个路径/配置操作。
      未安装时只有「选择路径 / 恢复默认路径」有意义，其余两个与框架文件相关，
      所以按安装状态禁用，而不是整体藏起来 —— 用户需要知道这些能力存在。
    -->
    <div class="panel-actions">
      <n-button size="small" secondary :disabled="!installed || working" @click="openPath">
        <template #icon><FolderOpenOutline width="15" height="15" /></template>
        {{ t('打开路径') }}
      </n-button>
      <n-button size="small" secondary :disabled="working" @click="choosePath">
        <template #icon><FolderOpenOutline width="15" height="15" /></template>
        {{ t('选择路径') }}
      </n-button>
      <n-button size="small" secondary :disabled="working" @click="restoreDefaultPath">
        <template #icon><RefreshOutline width="15" height="15" /></template>
        {{ t('恢复默认路径') }}
      </n-button>
      <n-button size="small" secondary :disabled="!installed || working" @click="configEditorOpen = true">
        <template #icon><SettingsOutline width="15" height="15" /></template>
        {{ t('配置管理') }}
      </n-button>
    </div>

    <!-- 安装 / 重装 / 备份 / 卸载 -->
    <div class="panel-install">
      <div class="panel-field">
        <span class="muted">{{ t('下载渠道') }}</span>
        <n-select v-model:value="installProvider" size="small" style="width: 190px" :options="[
          { label: t('Gitee（中国大陆推荐）'), value: 'gitee' },
          { label: 'GitHub', value: 'github' },
        ]" />
      </div>
      <div class="panel-buttons">
        <n-button size="small" type="primary" :loading="installing" :disabled="working"
          @click="install(installed)">
          <template #icon><AddCircleOutline width="15" height="15" /></template>
          {{ installed ? t('重新安装') : t('安装框架') }}
        </n-button>
        <n-button size="small" secondary :disabled="working || !installed" @click="backup">
          <template #icon><SaveOutline width="15" height="15" /></template>
          {{ t('创建备份') }}
        </n-button>
        <n-button size="small" secondary type="warning" :disabled="working || !installed" @click="uninstall">
          <template #icon><WarningOutline width="15" height="15" /></template>
          {{ t('卸载框架') }}
        </n-button>
      </div>
    </div>

    <!-- 备份列表 -->
    <div class="panel-backups">
      <div class="panel-backups-head">
        <b>{{ t('框架备份') }}</b>
        <n-button size="tiny" quaternary :disabled="working" @click="openBackups">
          <template #icon><FolderOpenOutline width="14" height="14" /></template>
          {{ t('打开备份目录') }}
        </n-button>
      </div>
      <div v-if="!backups.length" class="muted panel-empty">
        {{ t('暂无备份。卸载或重装前建议先创建备份。') }}
      </div>
      <div v-for="backup in backups" :key="backup" class="backup-row">
        <span class="mono backup-path" :title="backup">{{ backup }}</span>
        <span class="backup-acts">
          <n-button size="tiny" secondary @click="restore(backup)">{{ t('恢复') }}</n-button>
          <n-button size="tiny" quaternary type="error" :title="t('删除备份')" @click="removeBackup(backup)">
            <template #icon><TrashOutline width="14" height="14" /></template>
          </n-button>
        </span>
      </div>
    </div>

    <!-- 框架配置编辑器：与 MOD 配置共用同一组件，数据源切到 bepinex -->
    <ModConfigEditor v-model:show="configEditorOpen" source="bepinex" :title="t('BepInEx 配置')" />

    <!--
      平台引导：CrossOver 走安装前（先准备容器），原生 macOS 走安装后（给启动命令）。
      用 v-if 确保不显示时不残留 DOM。
    -->
    <BepInExSetupGuide v-if="guidePhase" :show="true" :config="config" :game="game"
      :phase="guidePhase" @update:show="(value: boolean) => { if (!value) guidePhase = null }"
      @confirmed="onGuideConfirmed" />

    <!-- 备份/恢复/卸载期间的全屏遮罩：这些操作都在整目录拷贝，不能被打断 -->
    <BusyOverlay :show="!!busyText" :description="busyText ?? undefined" />
  </n-spin>
</template>

<style scoped>
/*
 * 抽屉内是窄栏（约 520px），所有行都要能收缩：
 * 路径、版本号这类长文本一律单行省略 + title 保留全文。
 */
.panel-head {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding-bottom: 14px;
  border-bottom: 1px solid var(--oc2-row-line);
}
.panel-head-main {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 10px;
  min-width: 0;
}
.panel-title {
  font-size: 14px;
  font-weight: 600;
  color: var(--oc2-text-1);
}
.panel-version { font-size: 12px; }
.panel-runtime { font-size: 12px; }

.panel-actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  padding: 14px 0;
  border-bottom: 1px solid var(--oc2-row-line);
}

.panel-install {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 14px 0;
  border-bottom: 1px solid var(--oc2-row-line);
}
.panel-field {
  display: flex;
  align-items: center;
  gap: 10px;
}
.panel-buttons {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.panel-backups { padding-top: 14px; }
.panel-backups-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 8px;
}
.panel-empty { font-size: 12.5px; line-height: 1.6; }
.backup-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 7px 0;
  border-bottom: 1px dashed var(--oc2-row-line);
}
.backup-row:last-child { border-bottom: 0; }
.backup-path {
  flex: 1;
  min-width: 0;
  font-size: 11.5px;
  color: var(--oc2-text-3);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.backup-acts { display: flex; gap: 6px; flex: 0 0 auto; }
</style>
