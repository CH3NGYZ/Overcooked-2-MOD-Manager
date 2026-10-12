<script setup lang="ts">
import { Check, RefreshCw } from 'lucide-vue-next'
import type { GameInfo, InstalledMod, WinhttpState } from '../types'
import { t } from '../i18n'

type View = 'overview' | 'framework' | 'installed' | 'repositories' | 'sponsor'

const props = defineProps<{
  game?: GameInfo
  updateAvailableModCount: number
  installedMods: InstalledMod[]
  installableRepositoryCount: number
  repositoryCount: number
  repositoryRefreshActive: boolean
  currentWinhttp: WinhttpState
  nativeLoader?: boolean
  gameBlocked: boolean
  busy: string
  appIcon: string
  statusLabel: (status: WinhttpState) => string
  launchNormal: () => void
  toggleWinhttp: () => void
}>()

const emit = defineEmits<{ 'change-view': [view: View] }>()
</script>

<template>
  <section class="view-content">
    <div class="hero-row">
      <div>
        <div class="status-line"><span class="status-dot"
            :class="props.game?.game_exists ? 'success-bg' : 'danger-bg'"></span><span>{{
              props.game?.game_exists ? t('已识别厨房目录') : t('等待选择厨房目录') }}</span></div>
        <h2>{{ t('把 MOD 管理变成') }}<br /><em>{{ t('一次点击的事。') }}</em></h2>
        <p class="lead">{{ t('统一管理 BepInEx、Release 仓库和本地插件，安装过程会自动保护正在运行的厨房。') }}</p>
      </div>
      <div class="hero-orbit"><img :src="props.appIcon" alt="" /></div>
    </div>
    <div ref="tutorialOverviewMetrics" class="metric-grid">
      <div class="metric" data-tutorial="overview-bepinex" @click="emit('change-view', 'framework')"><span class="metric-label">BepInEx</span><strong>{{
        props.game?.bepinex.version ||
        (props.game?.bepinex.installed ? t('未知版本') : t('未安装')) }}</strong><span class="metric-note"
          :class="props.game?.bepinex.complete ? 'good' : ''">
          <Check v-if="props.game?.bepinex.complete" :size="13" /> {{ props.game?.bepinex.complete ? t('完整可用') :
            t('需要安装') }}
        </span></div>
      <button class="metric metric-button" data-tutorial="overview-framework-state"
          :disabled="props.gameBlocked || !props.game?.game_exists || !props.game?.bepinex.installed || props.currentWinhttp === 'missing' || !!props.busy" @click="props.toggleWinhttp"><span
          class="metric-label">{{ t('框架启用状态') }}</span><strong>{{ props.game?.bepinex.installed ? props.statusLabel(props.currentWinhttp) : t('框架未安装') }}</strong><span
          class="metric-note">{{ props.game?.kitchen_running ? t('操作已锁定') : !props.game?.bepinex.installed ? t('框架未安装') : props.currentWinhttp === 'conflict' ?
            t('点击清理冲突并启用框架') : props.currentWinhttp === 'modenabled' ? t('已启用框架, MOD 将会被加载') : props.currentWinhttp === 'moddisabled' ?
              t('已禁用框架, MOD 将不会被加载') : props.nativeLoader ? t('未找到框架加载器') : t('未找到 winhttp.dll') }}</span></button>
      <div class="metric" data-tutorial="overview-local-mods" @click="emit('change-view', 'installed')"><span class="metric-label">{{ t('本地 MOD')
          }}</span><strong class="metric-pair"><span class="metric-primary metric-update-count" :class="{ 'metric-updates-available': props.updateAvailableModCount > 0 }">{{ props.updateAvailableModCount
          }}</span><span class="metric-divider">/</span><span class="metric-secondary">{{ props.installedMods.length
          }}</span></strong><span class="metric-note">{{ t('有更新') }} / {{ t('已安装') }}</span></div>
      <div class="metric" data-tutorial="overview-cloud-mods" @click="emit('change-view', 'repositories')"><span class="metric-label metric-label-row"><span>{{ t('在线 MOD')
          }}</span><RefreshCw v-if="props.repositoryRefreshActive" class="spin-icon" :size="12" /></span><strong class="metric-pair"><span class="metric-primary">{{ props.installableRepositoryCount
          }}</span><span class="metric-divider">/</span><span class="metric-secondary">{{
            props.repositoryCount }}</span></strong><span class="metric-note">{{ t('可安装') }} / {{ t('总仓库')
            }}</span></div>
    </div>
    <div class="section-heading">
      <div>
        <h3>{{ t('厨房控制台') }}</h3>
      </div>
    </div>
    <div class="action-grid"><button class="action-card primary kitchen-launch-card"
        :disabled="props.gameBlocked || !props.game?.game_exists || !!props.busy" @click="props.launchNormal"><span
          class="action-icon">▶</span><span><b>{{ t('启动厨房') }}</b><small>{{ t('通过 Steam 启动游戏')
          }}</small></span><span>›</span></button></div>
  </section>
</template>
