<script setup lang="ts">
import { t } from '../i18n'
import { computed, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NLog, NSelect, NTag } from 'naive-ui'
import { BuildOutline, DocumentTextOutline, ServerOutline, TerminalOutline, TrashOutline } from '@vicons/ionicons5'
import sponsorImage from '../assets/collectionCode.jpg'
import AppModal from '../components/AppModal.vue'
import SettingsGroup from '../components/settings/SettingsGroup.vue'
import SettingsRow from '../components/settings/SettingsRow.vue'
import RecycleBinDialog from '../components/RecycleBinDialog.vue'
import { api } from '../services/tauri'
import type { RustLog } from '../types'

type LogKind = 'rust' | 'frontend' | 'crash'

const message = useMessage()
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
  { label: t('全部等级'), value: 'all' },
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
  rust: { get title() { return t('打开 Rust 后端日志') }, read: () => api.readRustLog() },
  frontend: { get title() { return t('打开前端日志') }, read: () => api.readFrontendLog() },
  crash: { get title() { return t('打开崩溃日志') }, read: () => api.readCrashLog() },
}

function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
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
  result.ok ? message.success(result.message || '已打开开发者工具') : message.error(result.message)
}

async function resetUi() {
  const result = await api.resetUi()
  result.ok ? message.success(result.message || '窗口状态已重置') : message.error(result.message)
}

/* ===== 回收站 ===== */

/*
 * 工具页只保留「打开回收站」入口。
 * 「删除 MOD 时优先放入回收站」是行为偏好，已移到「设置 → 通用设置」，
 * 与其它开关放在一起；工具页是维护/诊断工具集合，不该混入配置项。
 */

/** 回收站弹窗（只读浏览 + 恢复/彻底删除，交互在组件内）。 */
const recycleOpen = ref(false)
</script>

<template>
  <SettingsGroup :title="t('支持开发')">
    <div class="sponsor-card">
      <div class="sponsor-body">
        <img :src="sponsorImage" alt="赞助二维码" class="sponsor-img" />
        <div class="sponsor-text">
          <b>{{ t('觉得好用，请作者喝杯咖啡') }}</b>
          <span class="muted">{{ t('所有功能永久免费，赞助用于服务器与开发动力。感谢支持！') }}</span>
        </div>
      </div>
    </div>
  </SettingsGroup>

  <SettingsGroup :title="t('日志')">
    <SettingsRow :icon="ServerOutline" :title="t('打开 Rust 后端日志')" :description="t('安装、下载与文件操作的详细记录')"
      clickable @click="openLog('rust')" />
    <SettingsRow :icon="DocumentTextOutline" :title="t('打开前端日志')" :description="t('界面渲染与调用的记录')" clickable
      @click="openLog('frontend')" />
    <SettingsRow :icon="DocumentTextOutline" :title="t('打开崩溃日志')" :description="t('程序异常退出时的诊断信息')" clickable
      @click="openLog('crash')" />
  </SettingsGroup>

  <SettingsGroup :title="t('回收站')">
    <SettingsRow :icon="TrashOutline" :title="t('打开回收站')"
      :description="t('查看已删除的 MOD，可恢复到原位置或彻底删除')" clickable @click="recycleOpen = true" />
  </SettingsGroup>

  <SettingsGroup :title="t('高级')">
    <SettingsRow :icon="TerminalOutline" :title="t('打开开发者工具')" :description="t('WebView 开发者控制台（调试用）')" clickable
      @click="devtools" />
    <SettingsRow :icon="BuildOutline" :title="t('重置窗口状态')" :description="t('窗口位置或尺寸异常时恢复默认布局')" clickable
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
        <n-tag size="small" :bordered="false">{{ t('{count} 行', { count: filteredLineCount }) }}</n-tag>
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
      <n-log :log="filteredLog || '（该等级没有日志）'" :rows="60" :font-size="12" trim />
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
