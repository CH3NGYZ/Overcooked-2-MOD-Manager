<script setup lang="ts">
import { computed } from 'vue'
import { t } from '../i18n'
import { repositorySourceRegionLabel } from '../repositorySources'
import type { AppUpdateCheckResult, AppUpdateInfo } from '../types'
import type { TutorialProgressState } from '../tutorial/useTutorialRuntime'

const props = defineProps<{
  checking: boolean
  checkFailed: boolean
  skipped: boolean
  stage: string
  error: string
  phase: 'manifest' | 'download' | 'install' | 'skipped'
  check: AppUpdateCheckResult | null
  update: AppUpdateInfo | null
  countdown: number
  pageOpening: boolean
  pageError: string
  progress: TutorialProgressState
}>()
const emit = defineEmits<{ continue: []; 'manual-update': []; exit: [] }>()

const visibleChannels = computed(() => props.check?.channels.filter((channel) => channel.status !== 'skipped') || [])

function channelName(id: string) {
  if (id.startsWith('github-proxy:')) return `${t('GitHub 内置代理')} (${id.slice('github-proxy:'.length)})`
  return t(({ 'gitee-direct': 'Gitee 更新清单', 'github-direct': 'GitHub 更新清单' } as Record<string, string>)[id] || id)
}
function channelStatus(status: string) {
  return t(({ latest: '最新版', update_available: '有更新', failed: '失败', skipped: '未请求', cancelled: '已取消', checking: '检查中', downloading: '下载中', downloaded: '已下载' } as Record<string, string>)[status] || status)
}
function regionLabel(region: string) {
  if (region === 'detecting') return t('检测中')
  return repositorySourceRegionLabel(region)
}
</script>

<template>
  <div class="startup-overlay">
    <div v-if="checking && !skipped" class="startup-card">
      <div class="startup-spinner"></div>
      <h2>{{ stage || t('检查安装器更新') }}</h2>
      <p>{{ t('请耐心等待') }}</p>
      <div v-if="check" class="update-region">
        <span>{{ t('当前更新地区') }}</span><strong>{{ regionLabel(check.region) }}</strong>
      </div>
      <div v-if="phase === 'manifest' && visibleChannels.length" class="update-channel-statuses">
        <div v-for="channel in visibleChannels" :key="channel.id" class="update-channel-status">
          <span>{{ channelName(channel.id) }}</span><span :class="`update-channel-${channel.status}`">{{ channelStatus(channel.status) }}</span>
        </div>
      </div>
      <div v-if="phase === 'download' && update" class="update-release-log">
        <div class="update-release-head"><strong>{{ t('安装器更新') }} v{{ update.version }}</strong><small>{{ update.date || t('未知时间') }}</small></div>
        <p>{{ update.body || t('Release 没有更新日志。') }}</p>
      </div>
      <div v-if="phase === 'download' && progress.label" class="startup-download-progress">
        <div class="download-meta"><span>{{ progress.label }}<small v-if="progress.status"> · {{ progress.status }}</small></span><span><template v-if="!progress.indeterminate">{{ progress.percent.toFixed(0) }}%</template><small v-if="progress.speed"> · {{ progress.speed }}</small></span></div>
        <div class="progress" :class="{ 'progress-indeterminate': progress.indeterminate }"><i :style="{ width: progress.indeterminate ? '28%' : `${progress.percent}%` }"></i></div>
      </div>
    </div>
    <div v-else class="startup-card startup-failed">
      <div class="startup-exclamation">!</div>
      <h2>{{ error || t('检查更新失败，请重新启动MOD管理器。') }}</h2>
      <p v-if="skipped && countdown > 0">{{ countdown }} {{ t('秒后打开管理器仓库') }}</p>
      <p v-if="pageError" class="startup-update-error" role="alert">{{ pageError }}</p>
      <div v-if="skipped" class="startup-update-actions">
        <button class="secondary-button" @click="emit('continue')">{{ t('继续') }}</button>
        <button class="primary-button" :disabled="pageOpening" @click="emit('manual-update')">{{ t('手动更新') }}</button>
      </div>
      <button v-else class="primary-button" @click="emit('exit')">{{ t('确定') }}</button>
    </div>
  </div>
</template>
