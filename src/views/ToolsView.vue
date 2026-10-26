<script setup lang="ts">
import { t } from '../i18n'
import { nextTick, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NModal, NTag } from 'naive-ui'
import { BuildOutline, DocumentTextOutline, ServerOutline, TerminalOutline } from '@vicons/ionicons5'
import sponsorImage from '../assets/collectionCode.jpg'
import SettingsGroup from '../components/settings/SettingsGroup.vue'
import SettingsRow from '../components/settings/SettingsRow.vue'
import { api } from '../services/tauri'
import type { RustLog } from '../types'

type LogKind = 'rust' | 'frontend' | 'crash'

const message = useMessage()
const working = ref<Record<string, boolean>>({})
const logOpen = ref(false)
const logTitle = ref('')
const logPath = ref('')
const logContent = ref('')
let logPre: HTMLElement | null = null

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
    void nextTick(() => { if (logPre) logPre.scrollTop = logPre.scrollHeight })
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

  <SettingsGroup :title="t('高级')">
    <SettingsRow :icon="TerminalOutline" :title="t('打开开发者工具')" :description="t('WebView 开发者控制台（调试用）')" clickable
      @click="devtools" />
    <SettingsRow :icon="BuildOutline" :title="t('重置窗口状态')" :description="t('窗口位置或尺寸异常时恢复默认布局')" clickable
      @click="resetUi" />
  </SettingsGroup>

  <n-modal v-model:show="logOpen">
    <div class="log-modal">
      <div class="log-head">
        <b>{{ logTitle }}</b>
        <n-tag size="small" :bordered="false">{{ t('末尾 512 KiB') }}</n-tag>
        <n-button size="tiny" quaternary @click="logOpen = false">{{ t('关闭') }}</n-button>
      </div>
      <div class="muted mono log-path">{{ logPath }}</div>
      <pre ref="logPre" class="log-body">{{ logContent || '（空）' }}</pre>
    </div>
  </n-modal>
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
.log-modal {
  width: min(880px, calc(100vw - 80px));
  background: var(--oc2-card);
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  padding: 16px 18px;
}
.log-head {
  display: flex;
  align-items: center;
  gap: 10px;
}
.log-head b { flex: 1; }
.log-path { margin: 8px 0; }
.log-body {
  margin: 0;
  height: 60vh;
  overflow: auto;
  background: #14161a;
  border: 1px solid var(--oc2-rail-line);
  border-radius: 8px;
  padding: 12px;
  font-size: 11.5px;
  line-height: 1.55;
  color: #b8c4d0;
  white-space: pre-wrap;
  word-break: break-all;
}
</style>
