<script setup lang="ts">
import { X } from 'lucide-vue-next'
import { t } from '../i18n'
import type { ModOperations } from '../composables/useModOperations'

defineProps<{ ops: ModOperations; busy?: string; gameBlocked?: boolean }>()
</script>

<template>
  <div v-if="ops.manualInstallMode.value" class="modal-backdrop" @click.self="ops.manualInstallMode.value = null" @keydown.esc.stop="ops.manualInstallMode.value = null">
      <form class="modal manual-install-modal" @submit.prevent="ops.installOnlineMod()">
        <div class="modal-head">
          <h3>{{ t('手动安装模组') }}</h3>
          <button type="button" class="icon-button" :aria-label="t('关闭')" @click="ops.manualInstallMode.value = null"><X :size="17" /></button>
        </div>
        <div v-if="ops.manualInstallMode.value === 'choose'" class="modal-actions">
          <button type="button" class="secondary-button" @click="ops.chooseLocalMod()">{{ t('本地文件') }}</button>
          <button type="button" class="primary-button" @click="ops.manualInstallMode.value = 'online'">{{ t('在线文件') }}</button>
        </div>
        <template v-else>
          <label class="manual-install-field"><span>{{ t('下载直链') }}</span><input v-model="ops.manualInstallUrl.value" type="url" required :placeholder="t('请输入 DLL 或 ZIP 文件的下载直链')" :aria-label="t('下载直链')" /></label>
          <p class="muted-text">{{ t('仅支持 DLL 或 ZIP 文件，使用 32 线程直连下载。') }}</p>
          <div class="modal-actions">
            <button type="button" class="secondary-button" @click="ops.manualInstallMode.value = 'choose'">{{ t('返回') }}</button>
            <button type="submit" class="primary-button" :disabled="!!busy || gameBlocked || !ops.manualInstallUrl.value.trim()">{{ t('下载并安装') }}</button>
          </div>
        </template>
      </form>
    </div>
  
</template>
