<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
import { NButton, NProgress } from 'naive-ui'
import { X } from 'lucide-vue-next'
import { api, watchDownloadProgress } from '../services/tauri'
import type { DownloadProgress } from '../types'
import { useDashboard } from '../composables/useDashboardStore'

const { refresh } = useDashboard()
const progress = ref<DownloadProgress | null>(null)
let unlisten: (() => void) | null = null

const visible = computed(() => {
  const value = progress.value
  if (!value) return false
  if (value.finished) return false
  return value.downloaded > 0 || value.phase === 'connecting'
})
const percent = computed(() => {
  const value = progress.value
  if (!value?.total) return null
  return Math.min(100, Math.round((value.downloaded / value.total) * 100))
})
const speedText = computed(() => {
  const value = progress.value
  if (!value) return ''
  const mib = value.speed_bytes / (1024 * 1024)
  return mib >= 1 ? `${mib.toFixed(1)} MB/s` : `${Math.round(value.speed_bytes / 1024)} KB/s`
})

void watchDownloadProgress((value) => {
  progress.value = value
  if (value.finished) {
    progress.value = null
    void refresh()
  }
}).then((un) => { unlisten = un })

onUnmounted(() => { unlisten?.() })

async function cancel() {
  await api.cancelDownload().catch(() => undefined)
}
</script>

<template>
  <div v-if="visible && progress" class="download-strip">
    <div class="meta">
      <span>{{ progress.label }}</span>
      <span class="muted">
        {{ percent !== null ? `${percent}%` : '等待大小' }}
        · {{ speedText }}
        {{ progress.route === 'proxy' ? '· 代理' : progress.route === 'direct' ? '· 直连' : '' }}
      </span>
      <n-button quaternary size="tiny" @click="cancel"><X :size="14" />取消</n-button>
    </div>
    <n-progress type="line" :percentage="percent ?? 0" :show-indicator="false" :height="6" border-radius="3"
      :processing="percent === null" />
  </div>
</template>

<style scoped>
.download-strip {
  margin: 0 28px;
  padding: 10px 14px 12px;
  border: 1px solid #2c3540;
  border-radius: 8px;
  background: #1a1d22;
}
.meta {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-bottom: 7px;
  font-size: 12px;
}
.meta > span:first-child {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
