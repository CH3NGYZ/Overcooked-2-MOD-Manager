<script setup lang="ts">
import { nextTick, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NCard, NModal, NTag } from 'naive-ui'
import { FileText, RotateCcw, Terminal } from 'lucide-vue-next'
import sponsorImage from '../assets/collectionCode.jpg'
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
  rust: { title: 'Rust 后端日志', read: () => api.readRustLog() },
  frontend: { title: '前端日志', read: () => api.readFrontendLog() },
  crash: { title: '崩溃日志', read: () => api.readCrashLog() },
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
  <n-card title="支持开发" class="block">
    <div class="sponsor">
      <img :src="sponsorImage" alt="赞助二维码" class="sponsor-img" />
      <div class="muted">觉得好用可以请作者喝杯咖啡，感谢支持！</div>
    </div>
  </n-card>

  <n-card title="日志" class="block">
    <div class="acts">
      <n-button secondary :loading="!!working.rust" @click="openLog('rust')">
        <FileText :size="15" />Rust 后端日志
      </n-button>
      <n-button secondary :loading="!!working.frontend" @click="openLog('frontend')">
        <FileText :size="15" />前端日志
      </n-button>
      <n-button secondary :loading="!!working.crash" @click="openLog('crash')">
        <FileText :size="15" />崩溃日志
      </n-button>
    </div>
    <div class="muted hint">日志位于用户数据目录 logs/ 下，每份最多保留 5 个轮转文件。</div>
  </n-card>

  <n-card title="高级" class="block">
    <div class="acts">
      <n-button secondary @click="devtools"><Terminal :size="15" />开发者工具</n-button>
      <n-button secondary @click="resetUi"><RotateCcw :size="15" />重置窗口状态</n-button>
    </div>
    <div class="muted hint">重置窗口状态用于窗口位置或尺寸异常时恢复默认布局。</div>
  </n-card>

  <n-modal v-model:show="logOpen">
    <div class="log-modal">
      <div class="log-head">
        <b>{{ logTitle }}</b>
        <n-tag size="small" :bordered="false">末尾 512 KiB</n-tag>
        <n-button size="tiny" quaternary @click="logOpen = false">关闭</n-button>
      </div>
      <div class="muted mono log-path">{{ logPath }}</div>
      <pre ref="logPre" class="log-body">{{ logContent || '（空）' }}</pre>
    </div>
  </n-modal>
</template>

<style scoped>
.block { max-width: 760px; margin-bottom: 18px; }
.sponsor { display: flex; align-items: center; gap: 18px; }
.sponsor-img {
  width: 148px; height: 148px; object-fit: cover;
  border-radius: 10px; border: 1px solid #2c3540;
}
.acts { display: flex; gap: 10px; flex-wrap: wrap; }
.hint { margin-top: 10px; }
.log-modal {
  width: min(880px, calc(100vw - 80px));
  background: #1e2025; border: 1px solid #2c3540; border-radius: 12px;
  padding: 16px 18px;
}
.log-head { display: flex; align-items: center; gap: 10px; }
.log-head b { flex: 1; }
.log-path { margin: 8px 0; }
.log-body {
  margin: 0; height: 60vh; overflow: auto;
  background: #14161a; border: 1px solid #232a33; border-radius: 8px;
  padding: 12px; font-size: 11.5px; line-height: 1.55; color: #b8c4d0;
  white-space: pre-wrap; word-break: break-all;
}
</style>
