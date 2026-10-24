<script setup lang="ts">
import { X } from 'lucide-vue-next'
import { t } from '../i18n'
import type { Provider } from '../types'

defineProps<{ open: boolean; urls: { gitee: string; github: string }; busy?: string }>()
const emit = defineEmits<{ close: []; install: [provider: Provider] }>()
</script>

<template>
  <div v-if="open" class="modal-backdrop" @click.self="emit('close')">
      <div class="modal bepinex-channel-modal">
        <div class="modal-head">
          <div>
            <h3>{{ t('选择 BepInEx 安装渠道') }}</h3>
            <p class="muted-text">{{ t('国内用户请使用 Gitee 渠道，海外用户请使用 GitHub 渠道。') }}</p>
          </div><button class="icon-button" @click="emit('close')">
            <X :size="17" />
          </button>
        </div>
        <div class="bepinex-channel-grid">
          <button class="bepinex-channel-card" :disabled="!!busy" @click="emit('install', 'gitee')">
            <span class="bepinex-channel-icon gitee"><svg class="provider-icon" aria-hidden="true" viewBox="0 0 24 24">
                <use href="/icons.svg#gitee-icon"></use>
              </svg></span>
            <span class="bepinex-channel-copy"><b>{{ t('Gitee 渠道') }}</b><small>{{ urls.gitee
                }}</small><em>{{ t('直连 Release 文件') }}</em></span>
          </button>
          <button class="bepinex-channel-card" :disabled="!!busy" @click="emit('install', 'github')">
            <span class="bepinex-channel-icon github"><svg class="provider-icon" aria-hidden="true" viewBox="0 0 19 19">
                <use href="/icons.svg#github-icon"></use>
              </svg></span>
            <span class="bepinex-channel-copy"><b>{{ t('GitHub 渠道') }}</b><small>{{ urls.github
                }}</small><em>{{ t('直连 Release 文件') }}</em></span>
          </button>
        </div>
      </div>
    </div>

</template>
