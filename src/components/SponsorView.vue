<script setup lang="ts">
import { ExternalLink, FileText, Pencil, RotateCcw } from 'lucide-vue-next'
import { t } from '../i18n'

defineProps<{
  busy: string
  sponsorImage: string
  openDeveloperConsole: () => void
  openTutorialReplayMenu: () => void
  resetUi: () => void
  openRustLog: () => void
  openFrontendLog: () => void
  openCrashLog: () => void
}>()
</script>

<template>
  <section class="view-content sponsor-view">
    <div class="sponsor-panel">
      <h2>{{ t('觉得好用的话，请我喝杯咖啡吧！') }}</h2><img :src="sponsorImage" alt="赞助二维码" />
      <p>{{ t('感谢你的支持。') }}</p>
      <div class="button-row sponsor-actions"><button class="secondary-button" :disabled="!!busy"
          @click="openDeveloperConsole">
          <ExternalLink :size="15" />{{ t('打开开发者控制台') }}
        </button><button class="secondary-button" :disabled="!!busy" @click="openTutorialReplayMenu">
          <Pencil :size="15" />{{ t('重新观看教程') }}
        </button><button class="secondary-button" :disabled="!!busy" @click="resetUi">
          <RotateCcw :size="15" />{{ t('重置 UI') }}
        </button><button class="secondary-button" @click="openRustLog">
          <FileText :size="15" />{{ t('打开 Rust 日志') }}
        </button><button class="secondary-button" @click="openFrontendLog">
          <FileText :size="15" />{{ t('打开前端日志') }}
        </button><button class="secondary-button" @click="openCrashLog">
          <FileText :size="15" />{{ t('打开崩溃日志') }}
        </button></div>
    </div>
  </section>
</template>
