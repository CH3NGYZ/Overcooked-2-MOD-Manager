<script setup lang="ts">
import { t } from '../i18n'
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { NButton, NDrawer, NDrawerContent, NInput, NRadioButton, NRadioGroup, NSelect, NSwitch, NTag } from 'naive-ui'
import { ArrowUndoOutline, DownloadOutline, FolderOpenOutline, InformationCircleOutline } from '@vicons/ionicons5'
import { getVersion } from '@tauri-apps/api/app'
import PathEditor from '../components/PathEditor.vue'
import UpdatePrompt from '../components/UpdatePrompt.vue'
import AppModal from '../components/AppModal.vue'
import BepInExPanel from '../components/BepInExPanel.vue'
import SettingsGroup from '../components/settings/SettingsGroup.vue'
import SettingsRow from '../components/settings/SettingsRow.vue'
import DiyLevelPanel from '../components/settings/DiyLevelPanel.vue'
import { tagColorFor } from '../theme'
import { FOCUS_QUERY } from '../crossLink'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { useDashboard } from '../composables/useDashboardStore'
import { displayPath } from '../gameRuntime'
import type { AppUpdateCheckResult, GameRuntime, PermissionStatus, PublicConfig } from '../types'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'

const message = useNotify()
const route = useRoute()
const router = useRouter()
const { load, ensureLoaded, game, mods } = useDashboard()

const version = ref('')
const checking = ref(false)
const config = ref<PublicConfig | null>(null)
const savingPath = ref(false)

const pathDrawer = ref(false)
/** BepInEx 管理抽屉（右侧滑入）。 */
const bepinexDrawer = ref(false)
/** DIYLevel 管理抽屉（右侧滑入，与 BepInEx 同形态）。 */
const diyLevelDrawer = ref(false)
const tokenDrawer = ref(false)
const githubToken = ref('')
const giteeToken = ref('')
const savingTokens = ref(false)
const updateResult = ref<AppUpdateCheckResult | null>(null)
const updateInfo = ref<NonNullable<AppUpdateCheckResult['update']> | null>(null)
const showUpdate = ref(false)

/* ===== 游戏版本（macOS：CrossOver / 原生） ===== */

const isMacOS = computed(() => config.value?.platform === 'macos')
/** 与后端 `GameRuntime::default()` 一致：macOS 新装也默认原生。 */
const gameRuntime = computed<GameRuntime>(() => config.value?.game?.runtime ?? 'native')
const switchingRuntime = ref(false)

/**
 * 「游戏目录」这一条的说明直接显示当前目录。
 * 原来的说明句（「游戏与 BepInEx 框架所在目录……」）在第一次看过之后就
 * 没有信息量了，而「现在到底指向哪里」才是每天都要看一眼的东西。
 */
const gamePathLabel = computed(() => displayPath(game.value?.path) || t('overview.notSet'))

/**
 * 切换游戏运行时。
 *
 * 后端 `save_settings({ game_runtime })` 会先确认游戏已停止，再用
 * `switch_runtime` 同步调整路径与「是否原生」相关派生状态；完成后必须
 * 重新 `load()` —— 探测路径、BepInEx 包类型和启动方式都随之改变。
 */
async function switchRuntime(runtime: GameRuntime) {
  if (runtime === gameRuntime.value || switchingRuntime.value) return
  switchingRuntime.value = true
  try {
    config.value = await api.saveSettings({ game_runtime: runtime })
    await load()
    message.success(t('common.saved'))
  } catch (reason) {
    fail(reason)
  } finally {
    switchingRuntime.value = false
  }
}

/* ===== BepInEx 框架卡片摘要 ===== */

/** 框架是否已安装：判据是 `BepInEx/core` 下的核心程序集，不是目录存在。 */
const bepinexOk = computed(() => {
  const info = game.value?.bepinex
  return !!info && info.installed
})

/**
 * 卡片副标题：版本 + 是否启用。
 * 只放「一眼能看出状态」的信息，具体操作都在抽屉里。
 *
 * 未安装时要区分两种：完全没装，和「装过 MOD 留下了 BepInEx/plugins」。
 * 后者提示一句「已有 MOD 目录」，用户才知道装框架不会丢 MOD。
 */
const bepinexSummary = computed(() => {
  const info = game.value?.bepinex
  if (!bepinexOk.value) {
    return info?.has_residue
      ? t('framework.frameworkNotInstalledExistingPluginsConfigFoundAndWillBeKeptAfterInstall')
      : t('framework.notInstalledUseTheButtonOnTheRight')
  }
  const version = info?.version ? `${t('modsOnline.versions')} ${info.version}` : t('tools.versionUnknown')
  const enabled = game.value?.winhttp_state === 'modenabled'
  return `${version} · ${enabled ? t('overlays.frameworkEnabled') : t('overlays.frameworkDisabled')}`
})

/* ===== DIYLevel 框架卡片摘要（与 BepInEx 对齐） ===== */

/** DIYLevel 的稳定 GUID，与后端 `DIYLEVEL_GUID`、MOD 扫描一致。 */
const DIYLEVEL_GUID = 'dev.gua.overcooked.diylevel'

/**
 * 从 MOD 扫描结果里取 DIYLevel 那一项。
 *
 * 用 `mods` 而不是再调一次 `diy_level_info`：设置页已经拿到 Dashboard，
 * 多一次 IPC 只会让摘要比抽屉慢半拍。抽屉里才用专用命令（它还带地图数量）。
 */
const diyLevelMod = computed(() =>
  (mods.value ?? []).find((mod) =>
    !!mod.guid && mod.guid.trim().toLowerCase() === DIYLEVEL_GUID))

const diyLevelInstalled = computed(() => !!diyLevelMod.value)
const diyLevelEnabled = computed(() => diyLevelMod.value?.enabled === true)

/**
 * 卡片副标题：版本 + 启用状态，与 BepInEx 的行文一致。
 * 未安装时直接说明它是干什么的，避免用户不知道这个框架有什么用。
 */
const diyLevelSummary = computed(() => {
  if (!diyLevelInstalled.value) return t('framework.notInstalledUseTheButtonOnTheRight')
  const version = diyLevelMod.value?.version
    ? `${t('modsOnline.versions')} ${diyLevelMod.value.version}`
    : t('tools.versionUnknown')
  return `${version} · ${diyLevelEnabled.value ? t('overlays.frameworkEnabled') : t('overlays.frameworkDisabled')}`
})

/* ===== 权限：默认普通权限，仅在文件访问被拒时提权 ===== */
const permission = ref<PermissionStatus | null>(null)
const relaunching = ref(false)

async function refreshPermission() {
  permission.value = await api.permissionStatus().catch(() => null)
}

async function relaunchAdmin() {
  relaunching.value = true
  try {
    const result = await api.relaunchAsAdmin()
    if (!result.ok) message.error(translateBackendMessage(result.message))
    // 成功时进程会退出，这里的代码通常不会执行到
  } catch (reason) {
    fail(reason)
  } finally {
    relaunching.value = false
  }
}

onMounted(async () => {
  version.value = await getVersion().catch(() => '')
  config.value = await api.config().catch(() => null)
  await ensureLoaded()
  await refreshPermission()
  openRequestedPanel()
})

/**
 * 处理 `?focus=` 深链，例如总览页「安装框架」按钮跳过来要直接开抽屉。
 *
 * 这里不用 `useFocusHighlight`：那是给「页面上有 DOM 锚点的卡片」做高亮的，
 * 而设置页的目标是**抽屉**——抽屉没打开时 DOM 根本不存在，高亮无从谈起。
 * 所以按名字映射成「打开哪个抽屉」即可。
 */
const SETTINGS_PANELS: Record<string, () => void> = {
  bepinex: () => {
    bepinexDrawer.value = true
  },
  diylevel: () => {
    diyLevelDrawer.value = true
  },
  path: () => {
    pathDrawer.value = true
  },
  token: () => {
    tokenDrawer.value = true
  },
}

function openRequestedPanel() {
  const raw = route.query[FOCUS_QUERY]
  const id = typeof raw === 'string' ? raw : Array.isArray(raw) ? raw[0] : ''
  if (!id) return
  const open = SETTINGS_PANELS[id]
  // 消费后立刻抹掉，避免刷新页面时重复弹抽屉
  const { [FOCUS_QUERY]: _removed, ...rest } = route.query
  void router.replace({ name: route.name ?? undefined, query: rest })
  if (!open) return
  // 等一帧再开：Dashboard 刚到达，抽屉内容依赖 game/config
  window.setTimeout(open, 0)
}

function fail(reason: unknown) {
  message.error(backendMessageOf(reason))
}

/* ===== 路径管理：游戏目录 ===== */
async function openGameDir() {
  const result = await api.openGameDirectory()
  if (!result.ok) message.error(translateBackendMessage(result.message))
}

async function resetGamePath() {
  savingPath.value = true
  try {
    const info = await api.resetGamePath()
    message.success(info.path || t('common.reset'))
    await load()
  } catch (reason) { fail(reason) } finally { savingPath.value = false }
}

async function saveGamePath(path: string) {
  savingPath.value = true
  try {
    const info = await api.setGamePath(path)
    message.success(t(info.game_exists ? 'overview.verified' : 'common.theFolderIsMissingOrHasNoGameFiles'))
    pathDrawer.value = false
    await load()
  } catch (reason) { fail(reason) } finally { savingPath.value = false }
}

/* ===== 程序更新 ===== */

/**
 * 手动检测一次更新。
 *
 * 全部渠道失败时不再只弹一句「稍后重试」—— 原来的「更新设置」抽屉里有
 * 「打开发布页」补救入口，抽屉去掉后这个入口必须跟着搬到这里，
 * 否则用户在网络完全不通时无路可走。
 */
async function checkUpdate() {
  checking.value = true
  try {
    updateResult.value = await api.checkAppUpdate('zh-CN')
    if (updateResult.value.update) {
      // 检测到更新：与启动时相同的弹窗，更新/取消。
      updateInfo.value = updateResult.value.update
      showUpdate.value = true
    } else if (updateResult.value.all_failed) {
      openReleaseFallback()
    } else {
      message.success(t('dialogs.youAreUpToDate'))
    }
  } catch (reason) { fail(reason) } finally { checking.value = false }
}

/** 全部渠道失败时的补救入口：允许直接去发布页手动下载。 */
const updateFailedOpen = ref(false)

function openReleaseFallback() {
  updateFailedOpen.value = true
}

/* ===== 通用偏好 ===== */
async function savePreference(patch: Parameters<typeof api.saveSettings>[0], label: string) {
  try {
    config.value = await api.saveSettings(patch)
    message.success(label)
  } catch (reason) { fail(reason) }
}

/* ===== 性能：元数据并行读取 ===== */

/**
 * 下拉能给出的并发上限。
 *
 * 128 线程的机器上没必要列出一百多行；后端本来也会把超过核心数的存储值
 * clamp 回当前核心数，所以前端只保证「选项覆盖得了本机能选的范围」。
 */
const CONCURRENCY_LIMIT = 64
/**
 * 本机可用于读取文件的并行度。
 *
 * `navigator.hardwareConcurrency` 在部分 WebView（老系统、被裁剪的 WebView2）
 * 上拿不到，取不到就按 4 兜底 —— 与地图 info 的后端默认值一致。
 */
const cpuCores = Math.min(Math.max(navigator.hardwareConcurrency || 4, 1), CONCURRENCY_LIMIT)

/**
 * 两个下拉共用的选项：`0` = 全部核心，其余 `1..N`。
 *
 * 放在 computed 里是因为选项文案要走 `t()`：切语言时下拉里的「全部核心」
 * 必须跟着变，常量数组会一直停留在首次渲染的那一种语言。
 */
const concurrencyOptions = computed(() => [
  { label: t('settings.allCores'), value: 0 },
  ...Array.from({ length: cpuCores }, (_, index) => ({ label: String(index + 1), value: index + 1 })),
])

/**
 * 存储值 → 下拉显示值。
 *
 * `0` 表示「全部核心」，原样显示；其余按本机核心数 clamp。
 * **只改显示，不回写**：配置跟着用户走，换到 4 核机器上打开 8 核机器的配置时
 * 显示 4，但不能把 8 悄悄改写成 4 —— 回到 8 核机器上还要恢复 8。
 */
function displayConcurrency(value: number) {
  return value <= 0 ? 0 : Math.min(Math.max(value, 1), cpuCores)
}

/** MOD 与地图两侧默认都用全部核心（后端 `AppConfig` 的默认值同为 0）。 */
const modMetadataConcurrency = computed(() => displayConcurrency(config.value?.mod_metadata_concurrency ?? 0))
const mapInfoConcurrency = computed(() => displayConcurrency(config.value?.map_info_concurrency ?? 0))

/* ===== 删除行为：是否经过回收站 ===== */

/** 关闭「优先放入回收站」的风险确认弹窗。 */
const recycleRiskOpen = ref(false)

const recycleDeletedMods = computed(() => config.value?.recycle_deleted_mods !== false)

/**
 * 切换「删除 MOD 时优先放入回收站」。
 *
 * 打开是安全方向，直接保存。
 * 关闭会让之后删除的文件**不可恢复**，所以不立即保存，而是先弹风险确认；
 * 用户在确认框里再点一次才真正写入。
 */
function toggleRecycleDeletedMods(value: boolean) {
  if (value) {
    void savePreference(
      { recycle_deleted_mods: true },
      t('recycle.deletedModsWillGoToTheRecycleBin'),
    )
    return
  }
  recycleRiskOpen.value = true
}

/** 确认关闭：真正写入 false。 */
async function confirmDisableRecycle() {
  recycleRiskOpen.value = false
  await savePreference(
    { recycle_deleted_mods: false },
    t('recycle.deletedModsAreRemovedPermanently'),
  )
}

/* ===== 仅使用内置仓库源 ===== */

const builtinSourcesOnly = computed(() => config.value?.builtin_sources_only === true)
const switchingBuiltinSources = ref(false)

/**
 * 切换「仅使用内置仓库源」。
 *
 * 保存后后端会同步改写官方根源的地址，所以**必须**把返回的配置写回
 * `config.value` —— 否则开关状态要等下一次 Dashboard 读取才更新。
 */
async function toggleBuiltinSourcesOnly(value: boolean) {
  if (switchingBuiltinSources.value) return
  switchingBuiltinSources.value = true
  try {
    config.value = await api.saveSettings({ builtin_sources_only: value })
    message.success(value
      ? t('modsOnline.switchedToBundledSourcesRefreshToApply')
      : t('modsOnline.restoredOnlineSourcesRefreshToApply'))
  } catch (reason) {
    fail(reason)
  } finally {
    switchingBuiltinSources.value = false
  }
}

/* ===== 仅使用内置地图源 ===== */

const builtinMapSourcesOnly = computed(() => config.value?.builtin_map_sources_only === true)
const switchingBuiltinMapSources = ref(false)

/**
 * 切换「仅使用内置地图源」。
 *
 * 与仓库源开关**相互独立**：地图索引和仓库索引是两个文件，一个连不上
 * 不代表另一个也连不上，所以不合并成一个开关。地图源树不持久化，这个
 * 开关只决定下次刷新从哪里读索引。
 */
async function toggleBuiltinMapSourcesOnly(value: boolean) {
  if (switchingBuiltinMapSources.value) return
  switchingBuiltinMapSources.value = true
  try {
    config.value = await api.saveSettings({ builtin_map_sources_only: value })
    message.success(value
      ? t('modsOnline.switchedToBundledSourcesRefreshToApply')
      : t('modsOnline.restoredOnlineSourcesRefreshToApply'))
  } catch (reason) {
    fail(reason)
  } finally {
    switchingBuiltinMapSources.value = false
  }
}

/* ===== API 凭据 ===== */
/** 保存 Token：留空表示「不修改」，后端只在非空时覆盖。 */
async function saveTokens() {
  savingTokens.value = true
  try {
    config.value = await api.saveTokens({ github_token: githubToken.value, gitee_token: giteeToken.value })
    githubToken.value = ''
    giteeToken.value = ''
    message.success(t('common.saved'))
  } catch (reason) { fail(reason) } finally { savingTokens.value = false }
}

/** 清除已保存的 Token。 */
async function clearTokens() {
  savingTokens.value = true
  try {
    config.value = await api.saveTokens({ github_token: '', gitee_token: '', clear: true })
    githubToken.value = ''
    giteeToken.value = ''
    message.success(t('tools.cleared'))
  } catch (reason) { fail(reason) } finally { savingTokens.value = false }
}
</script>

<template>
  <!-- 路径管理 -->
  <SettingsGroup :title="t('settings.paths')">
    <!--
      游戏版本（仅 macOS）：决定探测哪个安装、装哪份 BepInEx、启动走哪条链路。
      放在游戏目录之上 —— 运行时是路径探测的前提，先选运行时再选目录才符合因果。
      原生在左、CrossOver 在右：原生是默认（macOS 版 BepInEx，不需要容器），
      CrossOver 走 Wine 容器里的 Windows 版，是给还没换原生框架的人留的选项。
    -->
    <SettingsRow v-if="isMacOS" :title="t('framework.gameVersion')"
      :description="t('framework.runTheWindowsBuildThroughCrossoverOrUseTheNativeMacosBuild')">
      <template #right>
        <n-radio-group :value="gameRuntime" size="small" :disabled="switchingRuntime"
          @update:value="(value: GameRuntime) => switchRuntime(value)">
          <n-radio-button value="native">{{ t('framework.native') }}</n-radio-button>
          <n-radio-button value="wine">{{ t('CrossOver') }}</n-radio-button>
        </n-radio-group>
      </template>
    </SettingsRow>
    <SettingsRow :title="t('settings.gameDirectory')" :description="gamePathLabel"
      :description-title="gamePathLabel">
      <template #right>
        <n-button size="small" secondary :disabled="savingPath" @click.stop="resetGamePath">
          <ArrowUndoOutline width="14" height="14" />{{ t('settings.resetPath') }}</n-button>
        <n-button size="small" secondary @click.stop="openGameDir">
          <FolderOpenOutline width="14" height="14" />{{ t('settings.openFolder') }}</n-button>
        <n-button size="small" type="primary" @click.stop="pathDrawer = true">
          {{ t('settings.choosePath') }}
        </n-button>
      </template>
    </SettingsRow>
    <!--
      BepInEx 框架：只展示版本与启用状态，全部管理收进右侧抽屉。
      框架管理和装一个 MOD 差不多复杂，不值得占常驻导航位。
    -->
    <SettingsRow :title="t('framework.bepinexFramework2')"
      :description="bepinexSummary">
      <template #right>
        <n-tag size="small" :bordered="false" class="state-tag" :color="tagColorFor(bepinexOk ? 'accent' : 'moderate')">
          {{ bepinexOk ? t('modsLocal.installed') : t('overview.notInstalled') }}
        </n-tag>
        <n-button size="small" :type="bepinexOk ? 'default' : 'primary'" :secondary="bepinexOk"
          @click.stop="bepinexDrawer = true">
          {{ bepinexOk ? t('framework.manage') : t('modsOnline.install') }}
        </n-button>
      </template>
    </SettingsRow>
    <!--
      DIYLevel 框架：与 BepInEx 完全同形态 —— 一行摘要 + 右侧抽屉管理。
      它是 MOD（GUID `dev.gua.overcooked.diylevel`）但承担框架角色，
      同一类东西两种交互会让人不知道该去哪儿找。
    -->
    <SettingsRow :title="t('framework.diylevelFramework2')"
      :description="diyLevelSummary">
      <template #right>
        <n-tag size="small" :bordered="false" class="state-tag"
          :color="tagColorFor(diyLevelInstalled ? 'accent' : 'moderate')">
          {{ diyLevelInstalled ? t('modsLocal.installed') : t('overview.notInstalled') }}
        </n-tag>
        <n-button size="small" :type="diyLevelInstalled ? 'default' : 'primary'"
          :secondary="diyLevelInstalled" @click.stop="diyLevelDrawer = true">
          {{ diyLevelInstalled ? t('framework.manage') : t('modsOnline.install') }}
        </n-button>
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!-- BepInEx 管理抽屉：从右向左滑入，内容由 BepInExPanel 承担 -->
  <n-drawer v-model:show="bepinexDrawer" :width="520" placement="right">
    <n-drawer-content :title="t('framework.bepinexFramework2')" closable :native-scrollbar="false">
      <BepInExPanel />
    </n-drawer-content>
  </n-drawer>

  <!-- DIYLevel 管理抽屉：与 BepInEx 同一宽度、同一位置、同一交互 -->
  <n-drawer v-model:show="diyLevelDrawer" :width="520" placement="right">
    <n-drawer-content :title="t('framework.diylevelFramework2')" closable :native-scrollbar="false">
      <DiyLevelPanel />
    </n-drawer-content>
  </n-drawer>

  <!--
    程序更新：只留一个「是否启动时自动检测」开关。
    手动检测不在这里 —— 它和版本号是同一件事，放在最下方「关于」的版本号旁边，
    那里用户才看得到「我现在是什么版本」。
  -->
  <SettingsGroup :title="t('settings.appUpdate')">
    <SettingsRow :title="t('overview.checkForUpdatesOnStartup')"
      :description="t('overview.checkForANewVersionWhenTheManagerStartsCanBeTurnedOffInSettings')">
      <template #right>
        <n-switch size="small" :value="config?.network.check_updates_on_start"
          @update:value="(value: boolean) => savePreference({ check_updates_on_start: value }, t('common.saved'))" />
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!-- 通用设置 -->
  <SettingsGroup :title="t('overview.general')">
    <SettingsRow :title="t('overview.autoDetectGamePath')" :description="t('overview.probeSteamLibrariesOnLaunchAndRefresh')">
      <template #right>
        <n-switch size="small" :value="config?.game?.auto_detect"
          @update:value="(value: boolean) => savePreference({ auto_detect: value }, t('common.saved'))" />
      </template>
    </SettingsRow>
    <SettingsRow :title="t('recycle.moveDeletedModsToTheRecycleBin')"
      :description="t('recycle.whenOffDeletedFilesAreRemovedPermanently')">
      <template #right>
        <!-- 开关本身不阻止点击：关闭时要先弹风险确认，由 toggle 决定是否真的写入 -->
        <n-switch size="small" :value="recycleDeletedMods"
          @update:value="toggleRecycleDeletedMods" />
      </template>
    </SettingsRow>
    <!--
      仅使用内置仓库源：官方入口不再按地区选 Gitee / GitHub 的 raw 直链，
      直接读管理器自带的快照。适合「官方入口在这台机器上连不上」或需要
      完全离线的场景；自定义仓库源不受影响。
    -->
    <SettingsRow :title="t('modsOnline.useBundledSourcesOnly')"
      :description="t('modsOnline.skipsTheGiteeGithubRawSourceLinksAndUsesTheSnapshotShippedWithTheManagerCustomSourcesAreNotAffected')">
      <template #right>
        <n-switch size="small" :value="builtinSourcesOnly" :loading="switchingBuiltinSources"
          @update:value="toggleBuiltinSourcesOnly" />
      </template>
    </SettingsRow>
    <!--
      仅使用内置地图源：与上面的仓库源开关并列但独立。
      地图索引和仓库索引是两个文件，一个连不上不代表另一个也连不上；
      合并成一个开关会让用户为了修复地图而关掉仓库的在线源。
    -->
    <SettingsRow :title="t('levelSet.useBundledMapSourcesOnly')"
      :description="t('levelSet.useBundledMapSourcesOnlyHint')">
      <template #right>
        <n-switch size="small" :value="builtinMapSourcesOnly" :loading="switchingBuiltinMapSources"
          @update:value="toggleBuiltinMapSourcesOnly" />
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!--
    性能：元数据并行读取。
    两项都只影响「同时读几个文件」，不改变任何结果，所以和开关类设置分开成组，
    也没有「恢复默认」—— 下拉里的「全部核心」就是 MOD 侧的默认值。
  -->
  <SettingsGroup :title="t('settings.performance')">
    <SettingsRow :title="t('settings.modMetadataConcurrency')"
      :description="t('settings.modMetadataConcurrencyHint')">
      <template #right>
        <n-select :value="modMetadataConcurrency" size="small" style="width: 160px" :options="concurrencyOptions"
          @update:value="(value: number) => savePreference({ mod_metadata_concurrency: value }, t('settings.modMetadataConcurrency'))" />
      </template>
    </SettingsRow>
    <SettingsRow :title="t('settings.mapInfoConcurrency')"
      :description="t('settings.mapInfoConcurrencyHint')">
      <template #right>
        <n-select :value="mapInfoConcurrency" size="small" style="width: 160px" :options="concurrencyOptions"
          @update:value="(value: number) => savePreference({ map_info_concurrency: value }, t('settings.mapInfoConcurrency'))" />
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!--
    关闭「优先放入回收站」的风险确认。
    这是不可逆行为的开关，所以把后果写清楚、要求用户再点一次确认，
    而不是让开关一拨就生效。
  -->
  <AppModal :show="recycleRiskOpen" :title="t('recycle.turnOffRecycleBinProtection')" max-width="min(520px, 92%)"
    :closable="false" @update:show="(value: boolean) => (recycleRiskOpen = value)">
    <div class="recycle-risk">
      <p class="recycle-risk-lead">{{ t('recycle.withThisOffDeletingAModRemovesItsFilesAndInstallManifestImmediatelyInsteadOfMovingThemToTheRecycleBin') }}</p>
      <p class="recycle-risk-note">{{ t('recycle.thisCannotBeUndoneDeletedModsAreUnrecoverableAndOverwritingAnInstallAlsoDiscardsThePreviousVersionConfirmThatYouReallyWantToBypassTheRecycleBin') }}</p>
    </div>
    <template #footer>
      <n-button size="small" type="error" @click="confirmDisableRecycle">
        {{ t('recycle.deleteDirectlyAnyway') }}
      </n-button>
      <n-button size="small" quaternary @click="recycleRiskOpen = false">
        {{ t('recycle.keepItOn') }}
      </n-button>
    </template>
  </AppModal>

  <!-- API 凭据：配置个人 Token 后，Release/API 请求直接带凭据，不再消耗匿名额度 -->
  <SettingsGroup :title="t('tools.apiCredentials')">
    <SettingsRow :title="t('tools.accessTokens')" :description="t('tools.withATokenApiRequestsUseItDirectlyInsteadOfTheAnonymousQuota')"
      clickable @click="tokenDrawer = true">
      <template #right>
        <n-tag size="small" :bordered="false" :type="config?.github_token_set ? 'success' : 'default'">
          GitHub{{ config?.github_token_set ? ' ✓' : '' }}
        </n-tag>
        <n-tag size="small" :bordered="false" :type="config?.gitee_token_set ? 'success' : 'default'">
          Gitee{{ config?.gitee_token_set ? ' ✓' : '' }}
        </n-tag>
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!-- 关于 -->
  <!--
    权限：应用默认以普通用户运行（启动不弹 UAC、文件拖放可用、
    更新时能正常关闭自己）。只有游戏目录不可写时才需要提权。
    macOS 不显示「以管理员身份重启」：那边没有 UAC，提权只能整进程
    以 root 重新启动，而这会带来 TCC 失效、配置与缓存变成 root 所有、
    更新器无法自替换；游戏目录与数据根都在用户目录下，本来就归当前用户，
    真正需要改的是目录权限本身。
  -->
  <SettingsGroup :title="t('dialogs.permissions')">
    <SettingsRow v-if="!isMacOS" :title="t('dialogs.runAsAdministrator')"
      :description="permission?.elevated ? t('dialogs.administratorRightsAreActive') : t('dialogs.onlyNeededWhenTheGameFolderIsNotWritable')">
      <template #right>
        <n-tag size="small" :bordered="false" :type="permission?.elevated ? 'success' : 'default'">
          {{ permission?.elevated ? t('dialogs.elevated') : t('dialogs.standard') }}
        </n-tag>
        <n-button v-if="!permission?.elevated" size="small" secondary type="primary" :loading="relaunching"
          @click.stop="relaunchAdmin">{{ t('dialogs.restartAsAdministrator') }}</n-button>
      </template>
    </SettingsRow>
    <SettingsRow :title="t('dialogs.gameFolderWritable')"
      :description="permission?.game_writable
        ? t('dialogs.modsCanBeInstalledAndRemovedDirectly')
        : (isMacOS ? t('dialogs.fixFolderPermissionsInFinder') : t('dialogs.administratorRightsAreRequiredToWrite'))">
      <template #right>
        <n-tag size="small" :bordered="false" :type="permission?.game_writable ? 'success' : 'warning'">
          {{ permission?.game_writable ? t('dialogs.writable') : t('dialogs.notWritable') }}
        </n-tag>
      </template>
    </SettingsRow>
  </SettingsGroup>

  <SettingsGroup :title="t('overview.about')">
    <SettingsRow :icon="InformationCircleOutline" :title="t('nav.oc2ModManager')" :description="t('overview.about') + ' · Tauri 2 / Vue 3 / Naive UI / Rust'">
      <template #right>
        <n-tag size="small" :bordered="false">v{{ version || '…' }}</n-tag>
        <!--
          手动检测更新放在版本号右边：用户想知道「我是不是最新」时才看版本，
          这两个动作是同一个意图。按钮只负责检测，安装走检测出的更新弹窗。
        -->
        <n-button size="small" :secondary="!checking" :loading="checking" @click.stop="checkUpdate">
          <template #icon><DownloadOutline width="14" height="14" /></template>
          {{ t('overview.check') }}
        </n-button>
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!--
    全部渠道检查失败：给一条能走的路。
    原来的「更新设置」抽屉里放着这个入口，抽屉删掉后放在这里，
    由「检测更新」按钮在 all_failed 时打开。
  -->
  <AppModal :show="updateFailedOpen" :title="t('dialogs.updateCheckFailed')" max-width="min(480px, 92%)"
    @update:show="(value: boolean) => (updateFailedOpen = value)">
    <p class="muted update-failed-note">{{ t('dialogs.allChannelsFailedDownloadManuallyFromTheReleasePage') }}</p>
    <template #footer>
      <n-button size="small" type="primary" @click="void api.openUpdateRepository(); updateFailedOpen = false">
        {{ t('dialogs.openReleasePage') }}
      </n-button>
      <n-button size="small" quaternary @click="updateFailedOpen = false">{{ t('recycle.cancel') }}</n-button>
    </template>
  </AppModal>

  <!-- 二级：选择路径 -->
  <n-drawer v-model:show="pathDrawer" :width="440" placement="right">
    <n-drawer-content :title="t('overview.chooseGameDirectory')" closable>
      <PathEditor :path="game?.path ?? ''" :saving="savingPath" @save="saveGamePath" />
      <div class="muted drawer-hint">{{ t('overview.theFolderMustContainOvercooked2ExeItIsVerifiedOnSave') }}</div>
    </n-drawer-content>
  </n-drawer>

  <!-- 二级：访问令牌 -->
  <n-drawer v-model:show="tokenDrawer" :width="460" placement="right">
    <n-drawer-content :title="t('tools.accessTokens')" closable>
      <div class="drawer-pref">
        <div>
          <b>GitHub Token</b>
          <div class="muted">{{ t('tools.sentAsAnAuthorizationBearerHeader') }}</div>
        </div>
        <n-tag v-if="config?.github_token_set" size="tiny" :bordered="false" type="success">{{ t('tools.configured') }}</n-tag>
      </div>
      <n-input v-model:value="githubToken" type="password" show-password-on="click"
        :placeholder="config?.github_token_set ? t('tools.savedLeaveBlankToKeep') : 'ghp_…'" />

      <div class="drawer-pref token-gap">
        <div>
          <b>Gitee Token</b>
          <div class="muted">{{ t('tools.sentAsAnAccessTokenHeader') }}</div>
        </div>
        <n-tag v-if="config?.gitee_token_set" size="tiny" :bordered="false" type="success">{{ t('tools.configured') }}</n-tag>
      </div>
      <n-input v-model:value="giteeToken" type="password" show-password-on="click"
        :placeholder="config?.gitee_token_set ? t('tools.savedLeaveBlankToKeep') : t('tools.pasteYourGiteePrivateToken')" />

      <div class="token-actions">
        <n-button type="primary" :loading="savingTokens" @click="saveTokens">{{ t('modsOnline.save') }}</n-button>
        <n-button v-if="config?.github_token_set || config?.gitee_token_set" quaternary type="error"
          :loading="savingTokens" @click="clearTokens">{{ t('tools.clearAll') }}</n-button>
      </div>

      <div class="muted drawer-hint">
        {{ t('tools.withTokensSetRequestsUseThemDirectlyAndOnlyFallBackToProxiesWhenTheTokenItselfIsRateLimited') }}
      </div>
    </n-drawer-content>
  </n-drawer>

  <UpdatePrompt v-model:show="showUpdate" :update="updateInfo" />
</template>

<style scoped>
/* 关闭回收站保护的风险说明：两句一重一轻，后果在前、补充说明在后 */
.recycle-risk {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.recycle-risk-lead {
  margin: 0;
  font-size: 13px;
  line-height: 1.65;
  color: var(--oc2-text-1);
}
.recycle-risk-note {
  margin: 0;
  font-size: 12.5px;
  line-height: 1.65;
  color: var(--oc2-text-3);
}
.drawer-hint { margin-top: 14px; }
.drawer-hint.danger { color: #ee8585; }
/* 更新检查失败弹窗的说明段：只有一句话，不要撑出多余留白 */
.update-failed-note { margin: 0; font-size: 13px; line-height: 1.65; }
.drawer-pref {
  display: flex; align-items: center; justify-content: space-between; gap: 14px;
  padding: 10px 0 14px; border-bottom: 1px dashed var(--oc2-rail-line); margin-bottom: 6px;
}
.drawer-pref b { font-size: 13.5px; }
/* Token 区块：第二组前面留出间距，保存按钮独立成行 */
.token-gap { margin-top: 18px; }
.token-actions {
  display: flex;
  gap: 8px;
  margin-top: 14px;
}
</style>
