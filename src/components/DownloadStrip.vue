<script setup lang="ts">
import { t } from '../i18n'
import { computed, onUnmounted, ref } from 'vue'
import { NButton, NProgress } from 'naive-ui'
import { CloseOutline } from '@vicons/ionicons5'
import { api, watchDownloadProgress } from '../services/tauri'
import type { DownloadProgress } from '../types'
import { useDashboard } from '../composables/useDashboardStore'
import { translateBackendMessage } from '../locale/backend'

const { refresh } = useDashboard()
const progress = ref<DownloadProgress | null>(null)
let unlisten: (() => void) | null = null

function percentValue(value: { downloaded: number; total?: number }): number {
  if (!value.total) return 0
  return Math.min(100, Math.round((value.downloaded / value.total) * 100))
}

/** 是否走「不确定进度」动画（有确定进度之前一律用 processing）。 */
const indeterminate = computed(() => {
  const value = progress.value
  if (!value || value.finished) return false
  if (!value.total) return true
  return (value.downloaded ?? 0) <= 0
})

/**
 * 卡片可见性：有任务就显示，直到 finished。
 *
 * 连接、切换、竞速阶段都要显示 —— 用户需要看到「正在连」而不是空白等待。
 */
const visible = computed(() => {
  const value = progress.value
  if (!value || value.finished) return false
  return true
})

const percent = computed(() => {
  if (indeterminate.value) return null
  const value = progress.value
  if (!value?.total) return null
  return percentValue(value)
})
const percentText = computed(() => {
  if (percent.value === null) return t('tools.preparing')
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
    parts.push(t('tools.sourcesRacing', { count: value.parallel_sources }))
  }
  if (value.attempt && value.attempt_total && value.attempt_total > 1) {
    parts.push(t('tools.attempt', { current: value.attempt, total: value.attempt_total }))
  }
  return parts.join(' · ')
})
const stageText = computed(() => {
  const value = progress.value
  if (!value) return ''
  switch (value.phase) {
    case 'connecting': return t('tools.connectingToDownloadSource')
    case 'switching': return t('tools.switchingToAFasterSource')
    case 'downloading': return t('tools.downloading')
    case 'finished': return t('tools.completed')
    default: return t('tools.preparing2')
  }
})
const routeText = computed(() => {
  const route = progress.value?.route
  return route === 'proxy' ? t('common.proxy') : route === 'direct' ? t('common.direct') : ''
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
          <!--
            标题来自后端的 `label`，它是**混合形态**：地图/MOD 资产传的是已本地化的
            名称（`RW 关卡合集`），直链与在线地图来源传的是 tag（`direct.onlineFile`、
            `levelSet.installMap`）。走 `translateBackendMessage` 统一处理 ——
            登记过的 tag 翻译成本地化文案，其它（含已经本地化的名字）原样返回。
          -->
          <h3 class="dl-title">{{ translateBackendMessage(progress.label) }}</h3>

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
              <CloseOutline width="13" height="13" />{{ t('common.cancelDownload') }}
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
  /* 层级取自 global.css 的浮层阶梯：低于标题栏弹层，高于页面内浮层 */
  z-index: var(--oc2-layer-download, 2600);
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
