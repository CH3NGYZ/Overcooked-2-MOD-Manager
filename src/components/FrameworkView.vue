<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import {
  AlertTriangle, Download, ExternalLink, FileCog, FolderOpen, HardDriveDownload,
  Pencil, RefreshCw, RotateCcw, ShieldCheck, Trash2,
} from 'lucide-vue-next'
import type { GameInfo, GameRuntime, PublicConfig } from '../types'
import { t } from '../i18n'
import { gameExecutableName } from '../gameRuntime'
import overcookedIcon from '../../src-tauri/icons/Overcooked2.icon.png'
import bepinexIcon from '../../src-tauri/icons/BepInEx.icon.png'
import PlatformSetupGuide from './PlatformSetupGuide.vue'

const props = defineProps<{
  game?: GameInfo
  config?: PublicConfig | null
  tutorialActive: boolean
  gameBlocked: boolean
  busy: string
  openDirectory: () => void
  chooseGameDirectory: () => void
  restoreDefaultGameDirectory: () => void
  detectGame: () => void
  changeGameRuntime: (runtime: GameRuntime) => Promise<void>
  openBepInExSetup: () => void
  openBepInExInstall: (reinstall: boolean) => void
  uninstallBepInEx: () => void
  openBepInExConfig: () => void
  openBackupDirectory: (backup?: string) => void
  backupBepInEx: () => void
  restoreBackup: (backup: string) => void
  deleteBackup: (backup: string) => void
}>()

const showPlatformGuide = ref(false)
const supportsRuntimeChoice = computed(() => props.config?.platform === 'linux' || props.config?.platform === 'macos')
const missingGame = computed(() => t('尚未找到 {executable}').replace('{executable}', gameExecutableName(props.config?.platform, props.config?.game.runtime)))

const shouldShowMacOSGuide = computed(() => {
  return !props.tutorialActive && props.config?.platform === 'macos'
    && props.config.game.runtime === 'wine' && !props.game?.game_exists
})

async function changeRuntime(event: Event) {
  const select = event.target as HTMLSelectElement
  const runtime = select.value
  if (runtime !== 'wine' && runtime !== 'native') return
  try {
    await props.changeGameRuntime(runtime)
  } catch {
    // The host reports the error; keep the select at the persisted mode.
  } finally {
    select.value = props.config?.game.runtime || 'wine'
  }
}

function handleGuideCompleted() {
  showPlatformGuide.value = false
  // Trigger game detection after user completes setup
  props.detectGame()
}

watch(shouldShowMacOSGuide, (show) => { showPlatformGuide.value = show }, { immediate: true })
</script>

<template>
  <section class="view-content">
    <div class="page-intro">
      <div>
        <p>{{ t('安装和维护框架文件，所有变更都会在厨房关闭时执行。') }}</p>
      </div>
      <div class="large-status" :class="props.game?.bepinex.complete ? 'good' : 'warn'">
        <ShieldCheck :size="22" /><span>{{ props.game?.bepinex.complete ? t('完整') : props.game?.bepinex.installed ?
          t('不完整') : t('未安装') }}</span>
      </div>
    </div>
    <div class="panel framework-panel game-directory-panel" data-tutorial="game-directory-card">
      <div class="framework-status">
        <div class="framework-icon">
          <img :src="overcookedIcon" alt="" />
        </div>
        <div class="framework-status-copy">
          <h3>{{ t('厨房目录') }}</h3>
          <div class="game-directory-path" data-tutorial="game-directory-path">
            <span :title="props.game?.path || undefined">{{ props.game?.path || missingGame }}</span>
          </div>
        </div>
      </div>
      <div v-if="supportsRuntimeChoice" class="game-runtime-choice" data-tutorial="game-runtime-choice">
        <label for="game-runtime-select">{{ t('厨房运行模式') }}</label>
        <select id="game-runtime-select" :value="props.config?.game.runtime" :disabled="!!props.busy || props.gameBlocked || props.tutorialActive" @change="changeRuntime">
          <option value="wine">{{ props.config?.platform === 'macos' ? t('Wine 版（CrossOver）') : t('Wine 版（Windows Steam）') }}</option>
          <option value="native">{{ props.config?.platform === 'macos' ? t('原生 macOS 版') : t('原生 Linux 版') }}</option>
        </select>
        <p>{{ t('切换模式会清空已选目录并重新检测，不会移动游戏或 MOD。') }}</p>
      </div>
      <div class="divider"></div>
      <div class="button-row game-directory-actions">
        <button class="primary-button" data-tutorial="game-directory-open" :disabled="!props.game?.path"
          @click="props.openDirectory">
          <ExternalLink :size="16" />{{ t('打开目录') }}
        </button>
        <button class="secondary-button" data-tutorial="game-directory-change" :disabled="!!props.busy || props.gameBlocked"
          @click="props.chooseGameDirectory">
          <Pencil :size="16" />{{ t('修改目录') }}
        </button>
        <button class="ghost-button" data-tutorial="game-directory-reset"
          :disabled="!!props.busy || props.gameBlocked || !!props.config?.game.auto_detect" @click="props.restoreDefaultGameDirectory">
          <RotateCcw :size="16" />{{ t('恢复默认') }}
        </button>
        <button class="ghost-button" data-tutorial="game-directory-rescan" :disabled="!!props.busy"
          @click="props.detectGame">
          <RefreshCw :size="16" />{{ t('重新检测') }}
        </button>
        <button v-if="props.config?.platform === 'macos' && props.config.game.runtime === 'wine'" class="ghost-button" :disabled="!!props.busy || props.tutorialActive" @click="showPlatformGuide = true">
          <ExternalLink :size="16" />{{ t('CrossOver 设置向导') }}
        </button>
      </div>
    </div>
    <div ref="tutorialFrameworkPanel" class="panel framework-panel" data-tutorial="bepinex-framework-card">
      <div class="framework-status">
        <div class="framework-icon">
          <img :src="bepinexIcon" alt="" />
        </div>
        <div class="framework-status-copy">
          <h3>{{ t('BepInEx框架') }}</h3>
          <span v-if="props.game?.bepinex.installed" class="framework-version">{{ props.game.bepinex.version || t('未检测到版本') }}</span>
          <p v-else>{{ t('未安装框架, 无法检测版本号') }}</p>
        </div><span class="badge" :class="props.game?.bepinex.complete ? 'success' : 'warning'">{{
          props.game?.bepinex.complete ? t('READY') : t('ACTION REQUIRED') }}</span>
      </div>
      <div class="divider"></div>
      <div class="button-row framework-actions"><button class="primary-button" :disabled="props.gameBlocked || !!props.busy"
          @click="props.openBepInExInstall(false)">
          <Download :size="16" />{{ t('安装 BepInEx') }}
        </button><button class="secondary-button" :disabled="props.gameBlocked || !!props.busy"
          @click="props.openBepInExInstall(true)">
          <RotateCcw :size="16" />{{ t('重新安装') }}
        </button><button class="danger-button"
          :disabled="props.gameBlocked || !props.game?.bepinex.installed || !!props.busy"
          @click="props.uninstallBepInEx">
          <Trash2 :size="16" />{{ t('卸载') }}
        </button><button class="secondary-button" data-tutorial="bepinex-open-config"
          :disabled="!props.game?.bepinex.has_config || !!props.busy"
          :title="!props.game?.bepinex.has_config ? t('配置不存在，请先运行一次游戏') : t('打开配置')"
          :aria-describedby="!props.game?.bepinex.has_config ? 'bepinex-config-hint' : undefined"
          @click="props.openBepInExConfig">
          <FileCog :size="16" />{{ t('打开配置') }}
        </button>
        <button v-if="supportsRuntimeChoice" class="ghost-button" :disabled="!!props.busy" @click="props.openBepInExSetup">
          <ExternalLink :size="16" />{{ t('Steam / Wine 启动设置') }}
        </button>
      </div>
      <p v-if="!props.game?.bepinex.has_config" id="bepinex-config-hint" class="guard-note">
        <AlertTriangle :size="15" />{{ t('配置不存在，请先运行一次游戏') }}
      </p>
      <p v-if="props.gameBlocked" class="guard-note">
        <AlertTriangle :size="15" />{{ t('《胡闹厨房2》正在运行，文件操作已禁用。') }}
      </p>
    </div>
    <div class="section-heading compact">
      <div>
        <h3>{{ t('框架备份') }}</h3>
      </div>
      <div class="button-row"><button class="ghost-button" :disabled="!!props.busy"
          @click="props.openBackupDirectory()">
          <FolderOpen :size="15" />{{ t('打开备份目录') }}
        </button><button class="ghost-button" :disabled="props.gameBlocked || !!props.busy"
          @click="props.backupBepInEx">
          <HardDriveDownload :size="15" />{{ t('立即备份') }}
        </button></div>
    </div>
    <div class="backup-list">
      <div v-for="backup in (props.game?.bepinex.backups || [])" :key="backup" class="backup-row">
        <div class="backup-icon">
          <HardDriveDownload :size="16" />
        </div>
        <div><b>{{ backup.split(/[\\/]/).pop() }}</b><span>{{ backup }}</span></div>
        <div class="button-row"><button class="ghost-button small-button" :title="t('打开备份目录')"
            :disabled="!!props.busy" @click="props.openBackupDirectory(backup)">
            <FolderOpen :size="14" />{{ t('打开') }}
          </button><button class="secondary-button small-button" data-tutorial="backup-restore"
            :disabled="props.gameBlocked || !!props.busy" @click="props.restoreBackup(backup)">
            <RotateCcw :size="14" />{{ t('恢复') }}
          </button><button class="danger-button small-button" data-tutorial="backup-delete"
            :disabled="props.gameBlocked || !!props.busy" @click="props.deleteBackup(backup)">
            <Trash2 :size="14" />{{ t('删除') }}
          </button></div>
      </div>
      <div v-if="!(props.game?.bepinex.backups?.length)" class="empty-state">{{ t('还没有框架备份') }}</div>
    </div>

    <!-- Platform Setup Guide -->
    <PlatformSetupGuide
      :visible="showPlatformGuide && !props.tutorialActive && !props.busy"
      :platform="props.config?.platform"
      :runtime="props.config?.game.runtime"
      @close="showPlatformGuide = false"
      @completed="handleGuideCompleted"
    />
  </section>
</template>
