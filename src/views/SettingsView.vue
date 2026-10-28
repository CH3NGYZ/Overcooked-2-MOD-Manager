<script setup lang="ts">
import { t } from '../i18n'
import { computed, onMounted, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NDrawer, NDrawerContent, NInput, NRadioButton, NRadioGroup, NSwitch, NTag } from 'naive-ui'
import { ArrowUndoOutline, DownloadOutline, FolderOpenOutline, InformationCircleOutline } from '@vicons/ionicons5'
import { getVersion } from '@tauri-apps/api/app'
import PathEditor from '../components/PathEditor.vue'
import UpdatePrompt from '../components/UpdatePrompt.vue'
import AppModal from '../components/AppModal.vue'
import BepInExPanel from '../components/BepInExPanel.vue'
import SettingsGroup from '../components/settings/SettingsGroup.vue'
import SettingsRow from '../components/settings/SettingsRow.vue'
import { tagColorFor } from '../theme'
import { api } from '../services/tauri'
import { useDashboard } from '../composables/useDashboardStore'
import type { AppUpdateCheckResult, GameRuntime, PermissionStatus, PublicConfig } from '../types'

const message = useMessage()
const { load, ensureLoaded, game } = useDashboard()

const version = ref('')
const checking = ref(false)
const config = ref<PublicConfig | null>(null)
const savingPath = ref(false)

const pathDrawer = ref(false)
/** BepInEx 管理抽屉（右侧滑入）。 */
const bepinexDrawer = ref(false)
const updateDrawer = ref(false)
const tokenDrawer = ref(false)
const githubToken = ref('')
const giteeToken = ref('')
const savingTokens = ref(false)
const updateResult = ref<AppUpdateCheckResult | null>(null)
const updateInfo = ref<NonNullable<AppUpdateCheckResult['update']> | null>(null)
const showUpdate = ref(false)

/* ===== 游戏版本（macOS：CrossOver / 原生） ===== */

const isMacOS = computed(() => config.value?.platform === 'macos')
const gameRuntime = computed<GameRuntime>(() => config.value?.game.runtime ?? 'wine')
const switchingRuntime = ref(false)

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
    message.success(t('已保存'))
  } catch (reason) {
    fail(reason)
  } finally {
    switchingRuntime.value = false
  }
}

/* ===== BepInEx 框架卡片摘要 ===== */

/** 框架是否已安装（以 BepInEx 目录是否完整为准）。 */
const bepinexOk = computed(() => {
  const info = game.value?.bepinex
  return !!info && info.installed
})

/**
 * 卡片副标题：版本 + 是否启用。
 * 只放「一眼能看出状态」的信息，具体操作都在抽屉里。
 */
const bepinexSummary = computed(() => {
  const info = game.value?.bepinex
  if (!bepinexOk.value) return t('尚未安装，点击右侧按钮安装')
  const version = info?.version ? `${t('版本')} ${info.version}` : t('版本未知')
  const enabled = game.value?.winhttp_state === 'modenabled'
  return `${version} · ${enabled ? t('框架已启用') : t('框架已禁用')}`
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
    if (!result.ok) message.error(result.message)
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
      t('删除的 MOD 将先移入回收站'),
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
    t('删除的 MOD 将直接删除，不进回收站'),
  )
}

/* ===== API 凭据 ===== */
/** 保存 Token：留空表示「不修改」，后端只在非空时覆盖。 */
async function saveTokens() {
  savingTokens.value = true
  try {
    config.value = await api.saveTokens({ github_token: githubToken.value, gitee_token: giteeToken.value })
    githubToken.value = ''
    giteeToken.value = ''
    message.success(t('已保存'))
  } catch (reason) { fail(reason) } finally { savingTokens.value = false }
}

/** 清除已保存的 Token。 */
async function clearTokens() {
  savingTokens.value = true
  try {
    config.value = await api.saveTokens({ github_token: '', gitee_token: '', clear: true })
    githubToken.value = ''
    giteeToken.value = ''
    message.success(t('已清除'))
  } catch (reason) { fail(reason) } finally { savingTokens.value = false }
}
</script>

<template>
  <!-- 路径管理 -->
  <SettingsGroup :title="t('路径管理')">
    <!--
      游戏版本（仅 macOS）：决定探测哪个安装、装哪份 BepInEx、启动走哪条链路。
      放在游戏目录之上 —— 运行时是路径探测的前提，先选运行时再选目录才符合因果。
      CrossOver 走 Wine 容器里的 Windows 版；原生走 macOS 版 BepInEx。
    -->
    <SettingsRow v-if="isMacOS" :title="t('游戏版本')"
      :description="t('CrossOver 运行 Windows 版，或使用 macOS 原生版')">
      <template #right>
        <n-radio-group :value="gameRuntime" size="small" :disabled="switchingRuntime"
          @update:value="(value: GameRuntime) => switchRuntime(value)">
          <n-radio-button value="wine">{{ t('CrossOver') }}</n-radio-button>
          <n-radio-button value="native">{{ t('原生') }}</n-radio-button>
        </n-radio-group>
      </template>
    </SettingsRow>
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
    <!--
      BepInEx 框架：只展示版本与启用状态，全部管理收进右侧抽屉。
      框架管理和装一个 MOD 差不多复杂，不值得占常驻导航位。
    -->
    <SettingsRow :title="t('BepInEx 框架管理')"
      :description="bepinexSummary">
      <template #right>
        <n-tag size="small" :bordered="false" class="state-tag" :color="tagColorFor(bepinexOk ? 'accent' : 'moderate')">
          {{ bepinexOk ? t('已安装') : t('未安装') }}
        </n-tag>
        <n-button size="small" :type="bepinexOk ? 'default' : 'primary'" :secondary="bepinexOk"
          @click.stop="bepinexDrawer = true">
          {{ bepinexOk ? t('管理') : t('安装') }}
        </n-button>
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!-- BepInEx 管理抽屉：从右向左滑入，内容由 BepInExPanel 承担 -->
  <n-drawer v-model:show="bepinexDrawer" :width="520" placement="right">
    <n-drawer-content :title="t('BepInEx 框架管理')" closable :native-scrollbar="false">
      <BepInExPanel />
    </n-drawer-content>
  </n-drawer>

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
    <SettingsRow :title="t('自动检测游戏路径')" :description="t('启动与刷新时自动探测 Steam 库')">
      <template #right>
        <n-switch size="small" :value="config?.game.auto_detect"
          @update:value="(value: boolean) => savePreference({ auto_detect: value }, t('已保存'))" />
      </template>
    </SettingsRow>
    <SettingsRow :title="t('删除 MOD 时优先放入回收站')"
      :description="t('关闭后删除的文件不会保留，无法恢复')">
      <template #right>
        <!-- 开关本身不阻止点击：关闭时要先弹风险确认，由 toggle 决定是否真的写入 -->
        <n-switch size="small" :value="recycleDeletedMods"
          @update:value="toggleRecycleDeletedMods" />
      </template>
    </SettingsRow>
  </SettingsGroup>

  <!--
    关闭「优先放入回收站」的风险确认。
    这是不可逆行为的开关，所以把后果写清楚、要求用户再点一次确认，
    而不是让开关一拨就生效。
  -->
  <AppModal :show="recycleRiskOpen" :title="t('关闭回收站保护')" max-width="min(520px, 92%)"
    @update:show="(value: boolean) => (recycleRiskOpen = value)">
    <div class="recycle-risk">
      <p class="recycle-risk-lead">{{ t('关闭后，管理器删除 MOD 时不再移入回收站，而是直接删除文件与安装清单。') }}</p>
      <p class="recycle-risk-note">{{ t('这个操作无法撤销，删除的 MOD 不能恢复；覆盖安装时旧版本也会被直接清除。请确认你确实想绕过回收站。') }}</p>
    </div>
    <template #footer>
      <n-button size="small" type="error" @click="confirmDisableRecycle">
        {{ t('仍然直接删除') }}
      </n-button>
      <n-button size="small" quaternary @click="recycleRiskOpen = false">
        {{ t('保持开启') }}
      </n-button>
    </template>
  </AppModal>

  <!-- API 凭据：配置个人 Token 后，Release/API 请求直接带凭据，不再消耗匿名额度 -->
  <SettingsGroup :title="t('API 凭据')">
    <SettingsRow :title="t('访问令牌')" :description="t('配置后 API 请求直接使用 Token，避免匿名限流')"
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
  -->
  <SettingsGroup :title="t('权限')">
    <SettingsRow :title="t('以管理员身份运行')"
      :description="permission?.elevated ? t('当前已具备管理员权限') : t('仅在游戏目录不可写时需要')">
      <template #right>
        <n-tag size="small" :bordered="false" :type="permission?.elevated ? 'success' : 'default'">
          {{ permission?.elevated ? t('已提权') : t('普通权限') }}
        </n-tag>
        <n-button v-if="!permission?.elevated" size="small" secondary type="primary" :loading="relaunching"
          @click.stop="relaunchAdmin">{{ t('以管理员重启') }}</n-button>
      </template>
    </SettingsRow>
    <SettingsRow :title="t('游戏目录可写')"
      :description="permission?.game_writable ? t('可以直接安装和卸载 MOD') : t('需要管理员权限才能写入')">
      <template #right>
        <n-tag size="small" :bordered="false" :type="permission?.game_writable ? 'success' : 'warning'">
          {{ permission?.game_writable ? t('可写') : t('不可写') }}
        </n-tag>
      </template>
    </SettingsRow>
  </SettingsGroup>

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
          <div class="muted">{{ t('启动管理器时检查新版本，可在设置中关闭') }}</div>
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

  <!-- 二级：访问令牌 -->
  <n-drawer v-model:show="tokenDrawer" :width="460" placement="right">
    <n-drawer-content :title="t('访问令牌')" closable>
      <div class="drawer-pref">
        <div>
          <b>GitHub Token</b>
          <div class="muted">{{ t('使用 Authorization: Bearer 请求头') }}</div>
        </div>
        <n-tag v-if="config?.github_token_set" size="tiny" :bordered="false" type="success">{{ t('已配置') }}</n-tag>
      </div>
      <n-input v-model:value="githubToken" type="password" show-password-on="click"
        :placeholder="config?.github_token_set ? t('已保存，留空则不修改') : 'ghp_…'" />

      <div class="drawer-pref token-gap">
        <div>
          <b>Gitee Token</b>
          <div class="muted">{{ t('使用 access_token 请求头') }}</div>
        </div>
        <n-tag v-if="config?.gitee_token_set" size="tiny" :bordered="false" type="success">{{ t('已配置') }}</n-tag>
      </div>
      <n-input v-model:value="giteeToken" type="password" show-password-on="click"
        :placeholder="config?.gitee_token_set ? t('已保存，留空则不修改') : t('粘贴 Gitee 私人令牌')" />

      <div class="token-actions">
        <n-button type="primary" :loading="savingTokens" @click="saveTokens">{{ t('保存') }}</n-button>
        <n-button v-if="config?.github_token_set || config?.gitee_token_set" quaternary type="error"
          :loading="savingTokens" @click="clearTokens">{{ t('清除全部') }}</n-button>
      </div>

      <div class="muted drawer-hint">
        {{ t('配置 Token 后，API 请求直接使用 Token；Token 自身被限流时才回退到代理。') }}
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
