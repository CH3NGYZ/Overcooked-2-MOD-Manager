<script setup lang="ts">
import { t } from '../i18n'
import { computed, onUnmounted, ref } from 'vue'
import { NButton, NProgress } from 'naive-ui'
import { CloseOutline } from '@vicons/ionicons5'
import { api, watchDownloadProgress } from '../services/tauri'
import type { DownloadProgress } from '../types'
import { useDashboard } from '../composables/useDashboardStore'

const { refresh } = useDashboard()
const progress = ref<DownloadProgress | null>(null)
let unlisten: (() => void) | null = null

/** 有实际字节才开始走进度；连接/切换/准备阶段走不确定动画。 */
const hasBytes = computed(() => (progress.value?.downloaded ?? 0) > 0 && !!progress.value?.total)
const visible = computed(() => {
  const value = progress.value
  if (!value || value.finished) return false
  // 准备/连接阶段也要显示卡片（加载动画），一旦开始下载则显示真实进度。
  return value.phase === 'connecting' || value.phase === 'switching' || hasBytes.value || (value.downloaded ?? 0) > 0
})
const percent = computed(() => {
  const value = progress.value
  if (!value?.total) return null
  return Math.min(100, Math.round((value.downloaded / value.total) * 100))
})
const percentText = computed(() => {
  if (percent.value === null) return t('准备中')
  return `${percent.value.toFixed(1)}%`
})
const speedText = computed(() => {
  const value = progress.value
  if (!value) return ''
  const mib = value.speed_bytes / (1024 * 1024)
  return mib >= 1 ? `${mib.toFixed(1)} MB/s` : `${Math.round(value.speed_bytes / 1024)} KB/s`
})
/** 竞速指示：并行源数 + 当前第几次尝试。 */
const raceText = computed(() => {
  const value = progress.value
  if (!value) return ''
  const parts: string[] = []
  if (value.parallel_sources && value.parallel_sources > 1) {
    parts.push(t('{count} 源竞速', { count: value.parallel_sources }))
  }
  if (value.attempt && value.attempt_total && value.attempt_total > 1) {
    parts.push(t('第 {current}/{total} 次尝试', { current: value.attempt, total: value.attempt_total }))
  }
  return parts.join(' · ')
})
const stageText = computed(() => {
  const value = progress.value
  if (!value) return ''
  switch (value.phase) {
    case 'connecting': return t('正在连接下载源…')
    case 'switching': return t('正在切换更快的源…')
    case 'downloading': return t('正在下载')
    case 'finished': return t('已完成')
    default: return t('准备中…')
  }
})
const routeText = computed(() => {
  const route = progress.value?.route
  return route === 'proxy' ? t('代理') : route === 'direct' ? t('直连') : ''
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
  // 立刻收起卡片：取消请求是异步的，后端也可能还没发出新的 finished 事件，
  // 等事件会让用户觉得按钮没反应。
  progress.value = null
  await api.cancelDownload().catch(() => undefined)
  // 取消后刷新一次，避免界面停在取消前的状态
  void refresh()
}
</script>

<template>
  <!-- 下载进度卡片：居中浮层，不占用页面布局 -->
  <Teleport to="body">
    <Transition name="dl-card">
      <div v-if="visible && progress" class="dl-mask">
        <section class="dl-card" role="status" aria-live="polite">
          <h3 class="dl-title">{{ progress.label }}</h3>

          <!-- 准备阶段：走 Naive 的不确定进度动画；开始收字节后才走真实百分比 -->
          <div class="dl-progress-row">
            <div class="dl-bar">
              <!-- processing 让 Naive 画条纹滚动动画，代表"进度未知" -->
              <n-progress v-if="percent === null" type="line" :percentage="100" :show-indicator="false"
                :height="6" :border-radius="3" processing />
              <n-progress v-else type="line" :percentage="percent" :show-indicator="false" :height="6"
                :border-radius="3" />
            </div>
            <span class="dl-percent">{{ percentText }}</span>
          </div>

          <div class="dl-meta">
            <span class="dl-stage">{{ stageText }}</span>
            <span v-if="raceText" class="dl-race">{{ raceText }}</span>
          </div>
          <div class="dl-speed">
            <span>{{ speedText }}</span>
            <span v-if="routeText" class="dl-route">{{ routeText }}</span>
          </div>

          <div class="dl-actions">
            <n-button secondary size="small" @click="cancel">
              <CloseOutline width="13" height="13" />{{ t('取消下载') }}
            </n-button>
          </div>
        </section>
      </div>
    </Transition>
  </Teleport>
</template>

<style scoped>
.dl-mask {
  position: fixed;
  /* 顶部让出自绘标题栏：下载卡片是全局浮层，但标题栏必须始终可拖动 */
  inset: var(--oc2-titlebar-height, 56px) 0 0 0;
  z-index: 2600;
  display: grid;
  place-items: center;
  background: rgba(0, 0, 0, .38);
  backdrop-filter: blur(1px);
}
.dl-card {
  width: min(420px, calc(100vw - 80px));
  padding: 20px 22px 16px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 16px;
  background: var(--oc2-card);
  box-shadow: 0 24px 70px rgba(0, 0, 0, .55);
}
.dl-title {
  margin: 0 0 16px;
  font-size: 17px;
  font-weight: 700;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.dl-progress-row {
  display: flex;
  align-items: center;
  gap: 12px;
}
.dl-bar {
  /* 只负责占位与宽度；轨道底色与填充由 n-progress 自己绘制 */
  flex: 1;
  min-width: 0;
  border-radius: 3px;
  overflow: hidden;
}
.dl-percent {
  flex: 0 0 auto;
  min-width: 54px;
  text-align: right;
  font-size: 12.5px;
  font-variant-numeric: tabular-nums;
  color: var(--oc2-text-2);
}
.dl-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 10px;
  font-size: 12px;
  color: var(--oc2-text-3);
}
.dl-stage { flex: 1; min-width: 0; }
.dl-race {
  flex: 0 0 auto;
  color: var(--oc2-accent);
  font-variant-numeric: tabular-nums;
}
.dl-speed {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-top: 4px;
  font-size: 11.5px;
  color: var(--oc2-text-4);
  font-variant-numeric: tabular-nums;
}
.dl-route { color: var(--oc2-text-3); }
.dl-actions {
  display: flex;
  justify-content: flex-end;
  margin-top: 14px;
}
.dl-card-enter-active,
.dl-card-leave-active {
  transition: opacity .16s ease;
}
.dl-card-enter-active .dl-card,
.dl-card-leave-active .dl-card {
  transition: transform .18s ease, opacity .16s ease;
}
.dl-card-enter-from,
.dl-card-leave-to {
  opacity: 0;
}
.dl-card-enter-from .dl-card,
.dl-card-leave-to .dl-card {
  transform: translateY(8px) scale(.98);
  opacity: 0;
}
</style>
