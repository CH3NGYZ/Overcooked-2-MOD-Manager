<script setup lang="ts">
import { t } from '../i18n'
import { ref } from 'vue'
import { useRouter } from 'vue-router'
import { NButton } from 'naive-ui'
import { BookOutline, BuildOutline, ChatbubblesOutline, CloudOfflineOutline, CloudUploadOutline, DocumentTextOutline, LogoDiscord, MapOutline, PersonOutline, RefreshOutline, ServerOutline, TerminalOutline, TrashOutline } from '@vicons/ionicons5'
import sponsorImage from '../assets/collectionCode.jpg'
import LogHistoryDialog from '../components/LogHistoryDialog.vue'
import { useNotify } from '../useNotify'
import { clearAvatarCache } from '../avatars'
import { clearChefThumbnailCache } from '../composables/useDiyChefThumbnails'
import { useManagerDataOperations } from '../composables/useManagerDataOperations'
import SettingsGroup from '../components/settings/SettingsGroup.vue'
import SettingsRow from '../components/settings/SettingsRow.vue'
import RecycleBinDialog from '../components/RecycleBinDialog.vue'
import TutorialManager from '../components/tutorial/TutorialManager.vue'
import { api } from '../services/tauri'
import type { LogKind } from '../types'
import { replayTutorial } from '../tutorial/engine'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'

const message = useNotify()
const router = useRouter()
const feedbackChannels = [
  { title: 'tools.arcadeModFeedbackGroup1', url: 'https://qm.qq.com/q/Uo7nhlbWiA' },
  { title: 'tools.arcadeModFeedbackGroup2', url: 'https://qm.qq.com/q/7CpekB9FYc' },
  { title: 'tools.modManagerFeedbackGroup', url: 'https://qm.qq.com/q/LN93oZTyUg' },
  { title: 'tools.modManagerFeedbackDiscord', url: 'https://discord.gg/rYx75pWERN', discord: true },
]
const tutorialManagerOpen = ref(false)
const logKind = ref<LogKind | null>(null)

function fail(reason: unknown) {
  message.error(backendMessageOf(reason))
}

async function devtools() {
  const result = await api.openDevtools()
  result.ok ? message.success(result.message ? translateBackendMessage(result.message) : t('tools.developerToolsOpened')) : message.error(translateBackendMessage(result.message))
}

async function resetUi() {
  const result = await api.resetUi()
  result.ok ? message.success(result.message ? translateBackendMessage(result.message) : t('tools.windowStateWasReset')) : message.error(translateBackendMessage(result.message))
}

/* ===== 回收站 ===== */

/*
 * 工具页只保留「打开回收站」入口。
 * 「删除 MOD 时优先放入回收站」是行为偏好，已移到「设置 → 通用设置」，
 * 与其它开关放在一起；工具页是维护/诊断工具集合，不该混入配置项。
 */

/** 回收站弹窗（只读浏览 + 恢复/彻底删除，交互在组件内）。 */
const recycleOpen = ref(false)

/* ===== 缓存维护 ===== */

/**
 * 清除头像缓存。
 *
 * 两层都要清：磁盘缓存（后端 `cache/avatars/`）与前端内存缓存。
 * 只清磁盘的话，本次会话仍在显示旧头像；只清内存的话，重新预取会立刻
 * 从磁盘读回同一张图 —— 用户会觉得「点了没反应」。
 */
const clearingAvatars = ref(false)
async function clearAvatars() {
  if (clearingAvatars.value) return
  clearingAvatars.value = true
  try {
    const removed = await api.clearRepositoryAvatars()
    clearAvatarCache()
    message.success(t('tools.clearedCachedAvatarS', { count: removed }))
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    clearingAvatars.value = false
  }
}

/**
 * 清除地图（关卡集）元数据缓存。
 *
 * 两级解析缓存都在后端；清除不撤销界面正在展示的共享列表。
 * 下一次实际扫描（例如在地图管理点「重新扫描」）才重新解析。
 */
const clearingLevels = ref(false)
async function clearLevels() {
  if (clearingLevels.value) return
  clearingLevels.value = true
  try {
    const removed = await api.clearLevelSetCache()
    message.success(t('tools.clearedCachedLevelInfoS', { count: removed }))
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    clearingLevels.value = false
  }
}

const clearingChefs = ref(false)
async function clearChefs() {
  if (clearingChefs.value) return
  clearingChefs.value = true
  try {
    const removed = await clearChefThumbnailCache()
    message.success(t('tools.clearedChefCache', { count: removed }))
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    clearingChefs.value = false
  }
}

const { managerDataDescription, managerDataBusy, clearingConfig, migratingData, selectingDataDirectory,
  confirmClearAllConfig, confirmMigrateManagerData } = useManagerDataOperations()
</script>

<template>
  <SettingsGroup :title="t('tools.supportDevelopment')">
    <div class="sponsor-card" data-tutorial="tools-sponsor">
      <div class="sponsor-body">
        <img :src="sponsorImage" alt="赞助二维码" class="sponsor-img" />
        <div class="sponsor-text">
          <b>{{ t('tools.buyTheAuthorACoffee') }}</b>
          <span class="muted">{{ t('tools.allFeaturesStayFreeSponsorshipsFundServersAndDevelopmentThankYou') }}</span>
        </div>
      </div>
    </div>
  </SettingsGroup>

  <SettingsGroup :title="t('tools.feedbackChannels')" data-testid="feedback-channels-group" data-tutorial="tools-feedback">
    <SettingsRow v-for="channel in feedbackChannels" :key="channel.url" :icon="channel.discord ? LogoDiscord : ChatbubblesOutline"
      :title="t(channel.title)" :description="t(channel.discord ? 'tools.discordFeedbackDescription' : 'tools.qqFeedbackGroupDescription')"
      clickable role="link" tabindex="0"
      @click="api.openReleasePage(channel.url).catch(fail)"
      @keydown.enter.prevent="api.openReleasePage(channel.url).catch(fail)"
      @keydown.space.prevent="api.openReleasePage(channel.url).catch(fail)" />
  </SettingsGroup>

  <SettingsGroup :title="t('mapPublish.title')" data-testid="map-publish-group">
    <SettingsRow :icon="CloudUploadOutline" :title="t('mapPublish.publishEntry')"
      :description="t('mapPublish.subtitle')" :description-title="t('mapPublish.subtitle')" clickable
      @click="router.push('/map-publish')" />
  </SettingsGroup>

  <SettingsGroup :title="t('tools.logs')" data-tutorial="tools-logs">
    <SettingsRow :icon="ServerOutline" :title="t('tools.openRustLog')" :description="t('tools.detailedRecordOfInstallsDownloadsAndFileOperations')"
      clickable @click="logKind = 'rust'" />
    <SettingsRow :icon="DocumentTextOutline" :title="t('tools.openFrontendLog')" :description="t('tools.uiRenderingAndIpcCalls')" clickable
      @click="logKind = 'frontend'" />
    <SettingsRow :icon="DocumentTextOutline" :title="t('tools.openCrashLog')" :description="t('tools.diagnosticsFromAbnormalExits')" clickable
      @click="logKind = 'crash'" />
  </SettingsGroup>

  <SettingsGroup :title="t('recycle.recycleBin')" data-tutorial="tools-recycle">
    <SettingsRow :icon="TrashOutline" :title="t('recycle.openRecycleBin')"
      :description="t('recycle.reviewDeletedModsRestoreThemOrDeletePermanently')" clickable @click="recycleOpen = true" />
  </SettingsGroup>

  <!--
    缓存维护：都是「取过一次就不再请求」的本地数据，过期或出错时只能手动清。
    头像：组织改名、代理换了张图之后仍显示旧头像；
    地图：缓存的键是 info 文件的内容哈希，解析结果被旧版本写错过就永远错下去。
    与回收站、日志并列放在工具页 —— 它们是维护动作，不是配置项。
  -->
  <SettingsGroup :title="t('tools.cache')" data-tutorial="tools-cache">
    <SettingsRow :icon="CloudOfflineOutline" :title="t('tools.clearAvatarCache')"
      :description="t('tools.deletesDownloadedRepositoryAvatarsTheyWillBeFetchedAgainTheNextTimeYouOpenTheModInstallPage')"
      clickable @click="clearAvatars" />
    <SettingsRow :icon="MapOutline" :title="t('tools.clearLevelCache')"
      :description="t('tools.deletesParsedMapInfoItWillBeReparsedTheNextTimeYouOpenTheMapPage')"
      clickable @click="clearLevels" />
    <SettingsRow :icon="PersonOutline" :title="t('tools.clearChefCache')"
      :description="t('tools.clearChefCacheDescription')" data-clear-chef-cache
      role="button" tabindex="0" :aria-busy="clearingChefs" clickable @click="clearChefs"
      @keydown.enter.prevent="clearChefs" @keydown.space.prevent="clearChefs" />
  </SettingsGroup>

  <SettingsGroup :title="t('tools.configuration')" data-testid="tools-configuration-group">
    <SettingsRow :icon="ServerOutline" :title="t('tools.managerDataPath')"
      :description="managerDataDescription" :description-title="managerDataDescription" data-tutorial="tools-data-migration" data-manager-data-path>
      <template #right>
        <n-button size="small" :disabled="managerDataBusy" :loading="selectingDataDirectory || migratingData"
          aria-haspopup="dialog" @click="confirmMigrateManagerData">{{ t('tools.migrateData') }}
        </n-button>
      </template>
    </SettingsRow>
    <SettingsRow :icon="TrashOutline" :title="t('tools.clearAllConfig')"
      :description="t('tools.clearAllConfigDescription')" :description-title="t('tools.clearAllConfigDescription')"
      class="clear-config-entry" data-tutorial="tools-data-reset" role="button" tabindex="0" aria-haspopup="dialog"
      :aria-label="t('tools.clearAllConfig')" :aria-busy="clearingConfig" :aria-disabled="managerDataBusy"
      clickable @click="confirmClearAllConfig" @keydown.enter.prevent="confirmClearAllConfig"
      @keydown.space.prevent="confirmClearAllConfig" />
  </SettingsGroup>

  <SettingsGroup :title="t('tools.advanced')" data-tutorial="tools-advanced">
    <SettingsRow :icon="TerminalOutline" :title="t('tools.openDeveloperTools')" :description="t('tools.webviewDeveloperConsoleDebugging')" clickable
      @click="devtools" />
    <SettingsRow :icon="BuildOutline" :title="t('tools.resetWindowState')" :description="t('tools.restoreTheDefaultLayoutWhenTheWindowIsMisplaced')" clickable
      @click="resetUi" />
    <!--
      教程默认只在首次启动播一次，之后想再看只能从这里进来。
      放在「高级」而不是单独开一组：它是一次性的界面动作，不是配置项。
      整行可点（与「打开回收站」同一个 SettingsRow 交互）：这类「点进去做一件事」
      的入口不该额外放按钮，否则一行动作被拆成「标题 + 按钮」两个焦点。
    -->
    <SettingsRow :icon="RefreshOutline" :title="t('tutorial.replay')"
      :description="t('tutorial.replayDescription')" clickable @click="replayTutorial" />
    <SettingsRow :icon="BookOutline" :title="t('tutorial.manager.title')"
      :description="t('tutorial.manager.description')" data-tutorial-manager-entry role="button" tabindex="0" aria-haspopup="dialog"
      clickable @click="tutorialManagerOpen = true" @keydown.enter.prevent="tutorialManagerOpen = true" @keydown.space.prevent="tutorialManagerOpen = true" />
  </SettingsGroup>

  <!-- 回收站浏览弹窗：唯一入口就在上面的「打开回收站」 -->
  <RecycleBinDialog v-model:show="recycleOpen" />
  <TutorialManager v-model:show="tutorialManagerOpen" />

  <LogHistoryDialog v-model:kind="logKind" />
</template>

<style scoped>
.sponsor-card {
  padding: 16px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-row);
}
.sponsor-body {
  display: flex;
  align-items: center;
  gap: 22px;
}
.sponsor-img {
  width: 260px;
  height: auto;
  border-radius: 10px;
  border: 1px solid var(--oc2-row-line);
  flex: 0 0 auto;
}
.sponsor-text {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.sponsor-text b {
  font-size: 14px;
}
.clear-config-entry :deep(.txt b) { color: var(--oc2-danger, #d03050); }
</style>
