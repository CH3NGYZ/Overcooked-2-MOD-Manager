<script setup lang="ts">
/**
 * DIYLevel 框架管理面板（放在设置页右侧抽屉里，与 BepInExPanel 同形态）。
 *
 * 为什么和 BepInEx 对齐：两者都是「框架」，用户对它们的心智模型一样 ——
 * 一行摘要看状态，点进去才有安装/开关/路径/卸载。之前 DIYLevel 是内联
 * 整块卡片、BepInEx 是抽屉，同一类东西两套交互，用户不知道该去哪儿找。
 *
 * DIYLevel 本质是一个 MOD（GUID `dev.gua.overcooked.diylevel`），但承担
 * 「框架」角色 —— 自制地图靠它加载。与另外两个概念严格区分：
 *   · BepInEx 是**加载器**（`bepinex::installed` 判核心程序集）
 *   · DIYLevel 是**内容框架 MOD**
 *   · 地图文件是用户数据，放在 `BepInEx/plugins/OC2DIYLevel/levels`
 *
 * 安装动作不自己实现下载：有在线仓库时直接跳到仓库卡片
 * （`/mods-online?focus=<仓库id>` 定位并高亮），没有仓库时如实说明只能手动装。
 */
import { computed, onMounted, ref } from 'vue'
import { useDialog } from 'naive-ui'
import { NButton, NSpin, NSwitch, NTag } from 'naive-ui'
import {
  AddCircleOutline, FolderOpenOutline, SaveOutline, TrashOutline, WarningOutline,
} from '@vicons/ionicons5'
import { api } from '../../services/tauri'
import { useNotify } from '../../useNotify'
import { pickI18n, t } from '../../i18n'
import { tagColorFor } from '../../theme'
import { formatBackupLabel } from '../../backupLabel'
import { backendMessageOf, translateBackendMessage } from '../../locale/backend'
import { markDialogBusy } from '../../dialogBusy'
import { useDashboard } from '../../composables/useDashboardStore'
import { useGameOperationGate } from '../../composables/useGameOperationGate'
import { useRepositoryLink } from '../../crossLink'
import BusyOverlay from '../BusyOverlay.vue'
import type { DiyLevelInfo, Repository } from '../../types'

/** DIYLevel 的稳定 GUID：与仓库源、安装清单、MOD 扫描用同一个标识。 */
const DIYLEVEL_GUID = 'dev.gua.overcooked.diylevel'

const message = useNotify()
const dialog = useDialog()
const { load, loading, config } = useDashboard()
const { blockModOperation } = useGameOperationGate()
const { linkToRepository } = useRepositoryLink()

const info = ref<DiyLevelInfo | null>(null)
const refreshing = ref(false)
const switching = ref(false)
const working = ref(false)

const installed = computed(() => info.value?.installed === true)
const enabled = computed(() => info.value?.enabled === true)
const backups = computed(() => info.value?.backups ?? [])
/**
 * 备份行展示数据：`时间 · 路径`（时间已转成当前时区）。
 *
 * 与 BepInExPanel 用同一个格式化函数，两处备份列表的观感必须一致。
 */
const backupRows = computed(() =>
  backups.value.map((path) => ({ path, time: formatBackupLabel(path).time })))
/** Only an active operation disables controls; game state is checked on click. */
const busy = computed(() => working.value)
/** 备份/恢复/卸载期间的全屏遮罩文案。 */
const busyText = ref<string | null>(null)

const repositories = computed<Repository[]>(() => config.value?.repositories ?? [])

/**
 * 能在线上提供 DIYLevel 的仓库（按 GUID 匹配）。
 *
 * 与依赖解析的「可安装来源」同一判据：必须声明同一 GUID、是 Release 模式、
 * 且已有缓存的 Release —— 三者缺一，点了也只有下载失败。
 */
const installableRepository = computed<Repository | undefined>(() =>
  repositories.value.find((repo) =>
    !!repo.plugin_guid
    && repo.plugin_guid.trim().toLowerCase() === DIYLEVEL_GUID
    && repo.download_mode === 'release'
    && (repo.cached_releases?.length ?? 0) > 0),
)

const repositoryName = computed(() =>
  installableRepository.value
    ? pickI18n(installableRepository.value.name_i18n, installableRepository.value.name)
    : '')

function fail(reason: unknown) {
  message.error(backendMessageOf(reason))
}

async function refresh() {
  refreshing.value = true
  try {
    info.value = await api.diyLevelInfo()
  } catch (reason) {
    fail(reason)
  } finally {
    refreshing.value = false
  }
}

/** 安装 / 重装：跳到 MOD 安装页并高亮对应仓库卡片，走同一条安装链路。 */
function openInstall() {
  if (blockModOperation()) return
  const repo = installableRepository.value
  if (!repo) {
    message.warning(t('framework.diylevelIsNotAvailableOnlineInstallItManuallyThenManageItHere'))
    return
  }
  linkToRepository(repo.id)
}

async function openDirectory() {
  try {
    const result = await api.openDiyLevelDirectory()
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) {
    fail(reason)
  }
}

/** 启用 / 禁用：走后端与 MOD 列表相同的 .dll 改名逻辑。 */
async function toggle(value: boolean) {
  if (blockModOperation()) return
  if (switching.value) return
  switching.value = true
  try {
    const result = await api.setDiyLevelEnabled(value)
    result.ok
      ? message.success(translateBackendMessage(result.message))
      : message.error(translateBackendMessage(result.message))
    await refresh()
    // MOD 列表里也显示这个插件，状态变了要同步刷新
    await load()
  } catch (reason) {
    fail(reason)
  } finally {
    switching.value = false
  }
}

/**
 * 卸载框架。
 *
 * 与 BepInEx 的「卸载框架」对齐：后端会**先备份整个 OC2DIYLevel 目录**
 * 再回收文件，所以这里只需一次确认，不再走 useModRemoval 的包确认链
 * —— 用户点的是框架级卸载，不是在 MOD 列表里删单个 DLL。
 */
function uninstall() {
  if (blockModOperation()) return
  const instance = dialog.warning({
    title: t('framework.uninstallDiylevelFramework'),
    content: t('framework.theWholeDiylevelFolderIsBackedUpBeforeUninstallingCustomMapsInsideAreMovedToTheRecycleBinTooContinue'),
    positiveText: t('overlays.uninstallFramework'),
    negativeText: t('recycle.cancel'),
    async onPositiveClick() {
      if (blockModOperation()) return false
      markDialogBusy(instance)
      working.value = true
      busyText.value = t('framework.backingUpAndUninstallingDiylevel')
      try {
        const result = await api.uninstallDiyLevel()
        result.ok
      ? message.success(translateBackendMessage(result.message))
      : message.error(translateBackendMessage(result.message))
        await refresh()
        await load()
      } catch (reason) {
        fail(reason)
      } finally {
        busyText.value = null
        working.value = false
      }
    },
  })
}

/** 创建一个 DIYLevel 目录备份。 */
async function backup() {
  if (blockModOperation()) return
  working.value = true
  busyText.value = t('framework.backingUpTheDiylevelFolder')
  try {
    const result = await api.backupDiyLevel()
    result.ok
      ? message.success(translateBackendMessage(result.message))
      : message.error(translateBackendMessage(result.message))
    await refresh()
  } catch (reason) {
    fail(reason)
  } finally {
    busyText.value = null
    working.value = false
  }
}

/** 恢复备份：先删现有 DIYLevel 目录，再整体还原（后端负责）。 */
function restore(backupPath: string) {
  if (blockModOperation()) return
  dialog.warning({
    title: t('framework.restoreDiylevelBackup'),
    content: t('framework.theCurrentDiylevelFolderWillBeDeletedThenReplacedWithThisBackupContinue'),
    positiveText: t('overlays.restore'),
    negativeText: t('recycle.cancel'),
    async onPositiveClick() {
      if (blockModOperation()) return false
      working.value = true
      busyText.value = t('framework.restoringDiylevelBackup')
      try {
        const result = await api.restoreDiyLevel(backupPath)
        result.ok
      ? message.success(translateBackendMessage(result.message))
      : message.error(translateBackendMessage(result.message))
        await refresh()
        await load()
      } catch (reason) {
        fail(reason)
      } finally {
        busyText.value = null
        working.value = false
      }
    },
  })
}

/** 删除一个备份目录。 */
function deleteBackup(backupPath: string) {
  dialog.warning({
    title: t('recycle.deleteBackup'),
    content: t('framework.thisBackupWillBeDeletedPermanentlyContinue'),
    positiveText: t('modsLocal.delete'),
    negativeText: t('recycle.cancel'),
    async onPositiveClick() {
      try {
        const result = await api.deleteDiyLevelBackup(backupPath)
        result.ok
      ? message.success(translateBackendMessage(result.message))
      : message.error(translateBackendMessage(result.message))
        await refresh()
      } catch (reason) {
        fail(reason)
      }
    },
  })
}

async function openBackups() {
  const result = await api.openBackupDirectory()
  if (!result.ok) message.error(translateBackendMessage(result.message))
}

/** 在文件管理器里打开**某一个**备份目录（与 BepInEx 面板的同名动作一致）。 */
async function openBackup(backupPath: string) {
  const result = await api.openBackupDirectory(backupPath)
  if (!result.ok) message.error(translateBackendMessage(result.message))
}

onMounted(() => { void refresh() })
</script>

<template>
  <n-spin :show="loading || refreshing">
    <!-- 状态摘要：开关 + 标题 + 版本 + 安装状态，与 BepInEx 面板同布局 -->
    <div class="panel-head">
      <div class="panel-head-main">
        <!-- 开关只在框架真正安装后才出现：没有可切换的插件 -->
        <n-switch v-if="installed" :value="enabled" :title="t('recycle.frameworkEnabledTheGameLoadsThisFramework')"
          :loading="switching" :disabled="busy" @update:value="toggle" />
        <span class="panel-title">{{ t('framework.diylevelFramework') }}</span>
        <span class="panel-version muted">{{ t('modsOnline.versions') }}：{{ info?.version || '—' }}</span>
        <n-tag size="small" :bordered="false" class="state-tag"
          :color="tagColorFor(installed ? (enabled ? 'accent' : 'muted') : 'muted')">
          {{ installed ? (enabled ? t('modsLocal.enabled2') : t('modsLocal.disabled')) : t('overview.notInstalled') }}
        </n-tag>
      </div>
    </div>

    <!--
      框架自身的操作，与 BepInEx 面板的「打开路径 / 配置管理」对齐。
      原来这里还并列一个「地图集：N」标签，已随地图管理拆页移除 ——
      地图列表将由独立的「地图管理」页面承担，这里只留框架本体相关动作。
    -->
    <div v-if="installed" class="panel-actions">
      <n-button size="small" secondary :disabled="busy" @click="openDirectory">
        <template #icon><FolderOpenOutline width="15" height="15" /></template>
        {{ t('framework.openMapFolder') }}
      </n-button>
    </div>

    <!-- 安装：未安装时只有这一个入口；已安装则是「重新安装 / 创建备份 / 卸载框架」 -->
    <div class="panel-install">
      <div class="panel-field">
        <span class="muted">{{ t('framework.installSource') }}</span>
        <span v-if="installableRepository" class="muted">{{ repositoryName }}</span>
        <span v-else class="muted">{{ t('framework.noRepositoryCanInstallItOnline') }}</span>
      </div>
      <div class="panel-buttons">
        <n-button size="small" type="primary" :disabled="busy || !installableRepository"
          @click="openInstall">
          <template #icon><AddCircleOutline width="15" height="15" /></template>
          {{ installed ? t('overlays.reinstall') : t('overlays.installFramework') }}
        </n-button>
        <template v-if="installed">
          <n-button size="small" secondary :disabled="busy" @click="backup">
            <template #icon><SaveOutline width="15" height="15" /></template>
            {{ t('overlays.createBackup') }}
          </n-button>
          <n-button size="small" secondary type="warning" :disabled="busy" @click="uninstall">
            <template #icon><WarningOutline width="15" height="15" /></template>
            {{ t('overlays.uninstallFramework') }}
          </n-button>
        </template>
      </div>
      <!-- 未安装时的说明：讲清装完之后能做什么，避免用户找不到维护入口 -->
      <p v-if="!installed" class="muted panel-hint">
        {{ installableRepository
          ? t('overlays.afterInstallingTheEnableSwitchConfigManagerAndBackupsAppearHere')
          : t('framework.notAvailableOnlineNoRepositoryDeclaresThisGuidInstallItManuallyThenManageItHere') }}
      </p>
    </div>

    <!-- 备份列表：与 BepInEx 面板同一套结构；没有备份且未安装时整块不出现 -->
    <div v-if="installed || backups.length" class="panel-backups">
      <div class="panel-backups-head">
        <b>{{ t('overlays.frameworkBackups') }}</b>
        <n-button size="tiny" quaternary :disabled="busy" @click="openBackups">
          <template #icon><FolderOpenOutline width="14" height="14" /></template>
          {{ t('overlays.openBackupFolder') }}
        </n-button>
      </div>
      <div v-if="!backups.length" class="muted panel-empty">
        {{ t('overlays.noBackupsYetCreateOneBeforeUninstallingOrReinstalling') }}
      </div>
      <div v-for="row in backupRows" :key="row.path" class="backup-row">
        <span class="backup-label" :title="row.path">
          <template v-if="row.time">
            <span class="backup-time">{{ row.time }}</span>
            <span class="backup-sep">·</span>
          </template>
          <span class="mono backup-path">{{ row.path }}</span>
        </span>
        <span class="backup-acts">
          <n-button size="tiny" secondary :disabled="busy" @click="openBackup(row.path)">{{ t('modsLocal.open') }}</n-button>
          <n-button size="tiny" secondary :disabled="busy" @click="restore(row.path)">{{ t('overlays.restore') }}</n-button>
          <n-button size="tiny" quaternary type="error" :disabled="busy" :title="t('recycle.deleteBackup')"
            @click="deleteBackup(row.path)">
            <template #icon><TrashOutline width="14" height="14" /></template>
          </n-button>
        </span>
      </div>
    </div>

    <!--
      地图集区块已移除：下一步会把「地图管理」做成与 MOD 管理同级的独立页面，
      这里再放一份列表会与那个页面重复。后端 level_packs / open_diy_level_directory
      仍保留，供新页面与总览卡片使用。
    -->

    <!-- 备份/恢复/卸载期间的全屏遮罩：这些操作都在整目录拷贝，不能被打断 -->
    <BusyOverlay :show="!!busyText" :description="busyText ?? undefined" />
  </n-spin>
</template>

<style scoped>
/*
 * 与 BepInExPanel 共用同一套 .panel-* 类名和观感：两个框架抽屉必须长得一样，
 * 否则用户会以为它们属于不同类型的对象。
 */
.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--oc2-row-line);
}
.panel-head-main { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.panel-title { font-size: 14px; font-weight: 600; color: var(--oc2-text-1); }
.panel-version { font-size: 12px; }
/* .state-tag 的字号由 global.css 的 .state-tag.n-tag 统一给，这里不再抄一份 */
.panel-metric { font-size: 11px; }

.panel-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 12px; }
.panel-install { margin-top: 14px; display: flex; flex-direction: column; gap: 10px; }
.panel-field { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.panel-buttons { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.panel-hint { font-size: 12px; margin: 0; line-height: 1.5; }

.panel-backups {
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--oc2-row-line);
}
.panel-backups-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.panel-empty { font-size: 12px; margin-top: 8px; line-height: 1.5; }

/* 备份行：与 BepInExPanel 完全一致的排版（`时间 · 路径` + 右侧按钮组） */
.backup-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  padding: 7px 0;
  border-bottom: 1px dashed var(--oc2-row-line);
}
.backup-row:last-child { border-bottom: 0; }
.backup-label {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  gap: 5px;
  font-size: 11.5px;
  color: var(--oc2-text-3);
}
.backup-time { flex: 0 0 auto; color: var(--oc2-text-2); }
.backup-sep { flex: 0 0 auto; color: var(--oc2-text-4); }
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
