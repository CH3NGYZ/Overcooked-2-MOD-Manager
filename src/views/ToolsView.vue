<script setup lang="ts">
import { t } from '../i18n'
import { computed, ref } from 'vue'
import { NLog, NSelect, NTag } from 'naive-ui'
import { BuildOutline, CloudOfflineOutline, DocumentTextOutline, MapOutline, ServerOutline, TerminalOutline, TrashOutline } from '@vicons/ionicons5'
import sponsorImage from '../assets/collectionCode.jpg'
import AppModal from '../components/AppModal.vue'
import { useNotify } from '../useNotify'
import { clearAvatarCache } from '../avatars'
import SettingsGroup from '../components/settings/SettingsGroup.vue'
import SettingsRow from '../components/settings/SettingsRow.vue'
import RecycleBinDialog from '../components/RecycleBinDialog.vue'
import { api } from '../services/tauri'
import type { RustLog } from '../types'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'

type LogKind = 'rust' | 'frontend' | 'crash'

const message = useNotify()
const working = ref<Record<string, boolean>>({})
const logOpen = ref(false)
const logTitle = ref('')
const logPath = ref('')
const logContent = ref('')

/**
 * 日志等级筛选。
 *
 * 日志行格式形如 `[时间][模块][INFO] 内容`，等级出现在第三个方括号里；
 * 少数行（如崩溃日志的堆栈）没有等级，归为 baseline —— 只在「全部」时显示，
 * 选了具体等级就把它们过滤掉，避免堆栈混在 INFO 里误导。
 */
type LogLevel = 'all' | 'TRACE' | 'DEBUG' | 'INFO' | 'WARN' | 'ERROR'

const logLevel = ref<LogLevel>('INFO')

const logLevelOptions = computed(() => [
  { label: t('rename.allLevels'), value: 'all' },
  { label: 'TRACE', value: 'TRACE' },
  { label: 'DEBUG', value: 'DEBUG' },
  { label: 'INFO', value: 'INFO' },
  { label: 'WARN', value: 'WARN' },
  { label: 'ERROR', value: 'ERROR' },
])

/** 单行日志的等级；读不出返回 null。 */
function levelOf(line: string): string | null {
  // 形如 [2026-10-27][14:04:13][app_lib::mods][INFO]
  const matches = line.match(/\[([A-Za-z]+)\]/g)
  if (!matches) return null
  for (const raw of matches) {
    const token = raw.slice(1, -1).toUpperCase()
    if (token === 'TRACE' || token === 'DEBUG' || token === 'INFO' || token === 'WARN' || token === 'ERROR') {
      return token
    }
  }
  return null
}

/**
 * 按等级过滤后的日志正文。
 *
 * `all` 原样返回；选具体等级时保留该等级及以上（等级越高越严重），
 * 这是日志查看器的常规语义 —— 选 INFO 时你不会想丢掉 WARN 和 ERROR。
 */
const filteredLog = computed(() => {
  const content = logContent.value
  if (!content) return ''
  if (logLevel.value === 'all') return content
  const order: Record<string, number> = { TRACE: 0, DEBUG: 1, INFO: 2, WARN: 3, ERROR: 4 }
  const minimum = order[logLevel.value]
  return content
    .split('\n')
    .filter((line) => {
      const level = levelOf(line)
      if (level === null) return false
      return order[level] >= minimum
    })
    .join('\n')
})

/** 当前筛选命中的行数，显示在标题旁帮助判断是否筛过头了。 */
const filteredLineCount = computed(() => (filteredLog.value ? filteredLog.value.split('\n').length : 0))

const logMeta: Record<LogKind, { title: string; read: () => Promise<RustLog> }> = {
  rust: { get title() { return t('tools.openRustLog') }, read: () => api.readRustLog() },
  frontend: { get title() { return t('tools.openFrontendLog') }, read: () => api.readFrontendLog() },
  crash: { get title() { return t('tools.openCrashLog') }, read: () => api.readCrashLog() },
}

function fail(reason: unknown) {
  message.error(backendMessageOf(reason))
}

async function openLog(kind: LogKind) {
  const meta = logMeta[kind]
  working.value[kind] = true
  try {
    const log = await meta.read()
    logTitle.value = meta.title
    logPath.value = log.path
    logContent.value = log.content
    logOpen.value = true
  } catch (reason) { fail(reason) } finally { working.value[kind] = false }
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
 * 两层都在后端，这里没有前端副本要清：地图页每次进入都重新调 `api.levelSets()`，
 * 只是那次调用会被缓存拦住。清完之后再进地图页就会真正重解析。
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
</script>

<template>
  <SettingsGroup :title="t('tools.supportDevelopment')">
    <div class="sponsor-card">
      <div class="sponsor-body">
        <img :src="sponsorImage" alt="赞助二维码" class="sponsor-img" />
        <div class="sponsor-text">
          <b>{{ t('tools.buyTheAuthorACoffee') }}</b>
          <span class="muted">{{ t('tools.allFeaturesStayFreeSponsorshipsFundServersAndDevelopmentThankYou') }}</span>
        </div>
      </div>
    </div>
  </SettingsGroup>

  <SettingsGroup :title="t('tools.logs')">
    <SettingsRow :icon="ServerOutline" :title="t('tools.openRustLog')" :description="t('tools.detailedRecordOfInstallsDownloadsAndFileOperations')"
      clickable @click="openLog('rust')" />
    <SettingsRow :icon="DocumentTextOutline" :title="t('tools.openFrontendLog')" :description="t('tools.uiRenderingAndIpcCalls')" clickable
      @click="openLog('frontend')" />
    <SettingsRow :icon="DocumentTextOutline" :title="t('tools.openCrashLog')" :description="t('tools.diagnosticsFromAbnormalExits')" clickable
      @click="openLog('crash')" />
  </SettingsGroup>

  <SettingsGroup :title="t('recycle.recycleBin')">
    <SettingsRow :icon="TrashOutline" :title="t('recycle.openRecycleBin')"
      :description="t('recycle.reviewDeletedModsRestoreThemOrDeletePermanently')" clickable @click="recycleOpen = true" />
  </SettingsGroup>

  <!--
    缓存维护：都是「取过一次就不再请求」的本地数据，过期或出错时只能手动清。
    头像：组织改名、代理换了张图之后仍显示旧头像；
    地图：缓存的键是 info 文件的内容哈希，解析结果被旧版本写错过就永远错下去。
    与回收站、日志并列放在工具页 —— 它们是维护动作，不是配置项。
  -->
  <SettingsGroup :title="t('tools.cache')">
    <SettingsRow :icon="CloudOfflineOutline" :title="t('tools.clearAvatarCache')"
      :description="t('tools.deletesDownloadedRepositoryAvatarsTheyWillBeFetchedAgainTheNextTimeYouOpenTheModInstallPage')"
      clickable @click="clearAvatars" />
    <SettingsRow :icon="MapOutline" :title="t('tools.clearLevelCache')"
      :description="t('tools.deletesParsedMapInfoItWillBeReparsedTheNextTimeYouOpenTheMapPage')"
      clickable @click="clearLevels" />
  </SettingsGroup>

  <SettingsGroup :title="t('tools.advanced')">
    <SettingsRow :icon="TerminalOutline" :title="t('tools.openDeveloperTools')" :description="t('tools.webviewDeveloperConsoleDebugging')" clickable
      @click="devtools" />
    <SettingsRow :icon="BuildOutline" :title="t('tools.resetWindowState')" :description="t('tools.restoreTheDefaultLayoutWhenTheWindowIsMisplaced')" clickable
      @click="resetUi" />
  </SettingsGroup>

  <!-- 回收站浏览弹窗：唯一入口就在上面的「打开回收站」 -->
  <RecycleBinDialog v-model:show="recycleOpen" />

  <!--
    日志查看：fill 让弹窗一打开就是允许的最大尺寸。
    日志长度在打开前未知，随内容自适应会让弹窗先小后大地跳一次；
    固定成上限后，路径与 n-log 的位置在加载前后都不动。
  -->
  <AppModal :show="logOpen" card-class="log-dialog" :title="logTitle" fill max-width="min(880px, 90%)"
    @update:show="(value: boolean) => (logOpen = value)">
    <template #header-extra>
      <div class="log-head-extra">
        <!-- 等级筛选：默认 INFO，去掉 TRACE/DEBUG 的噪音 -->
        <n-select v-model:value="logLevel" size="small" class="log-level" :options="logLevelOptions" />
        <n-tag size="small" :bordered="false">{{ t('rename.lines', { count: filteredLineCount }) }}</n-tag>
      </div>
    </template>
    <div class="muted mono log-path">{{ logPath }}</div>
    <!--
      不传 language：日志是纯文本，传了会走 hljs 高亮并因缺少 hljs 实例告警。
      n-log 只有 rows 一个尺寸入口（等价于固定行高），所以外面套一层 flex
      容器让它吃掉剩余高度，rows 给足够大的值充当上限 —— 弹窗本身已由
      fill 固定尺寸，内容不会因此变高。
    -->
    <div class="log-body">
      <n-log :log="filteredLog || t('tools.noLogsAtThisLevel')" :rows="60" :font-size="12" trim />
    </div>
  </AppModal>
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
.log-path { margin: 0 0 10px; }
/*
 * 日志正文：吃掉弹窗内容区的剩余高度。
 * n-log 的高度由 rows 决定，这里只负责让它在一个可收缩的容器里，
 * 滚动发生在 n-log 自己的滚动区（Naive 内置），不额外嵌套滚动层。
 */
.log-body {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  overflow: hidden;
}
.log-body :deep(.n-log) { flex: 1 1 auto; min-height: 0; }
/* 标题右侧：等级选择 + 行数统计 */
.log-head-extra { display: flex; align-items: center; gap: 8px; }
.log-level { width: 110px; }
</style>
