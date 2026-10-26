<script setup lang="ts">
import { NConfigProvider, NDialogProvider, NMessageProvider, NGlobalStyle, zhCN, dateZhCN } from 'naive-ui'
import { getVersion } from '@tauri-apps/api/app'
import { onMounted, ref } from 'vue'
import AppSidebar from './components/AppSidebar.vue'
import DownloadStrip from './components/DownloadStrip.vue'
import UpdatePrompt from './components/UpdatePrompt.vue'
import { startGameWatch } from './composables/useDashboardStore'
import { api } from './services/tauri'
import type { AppUpdateInfo } from './types'
import { darkTheme, themeOverrides } from './theme'

const version = ref('')
const updateInfo = ref<AppUpdateInfo | null>(null)
const showUpdate = ref(false)

onMounted(() => {
  startGameWatch()
  getVersion().then((value) => { version.value = value }).catch(() => undefined)
  // 启动更新检查：发现新版本才弹窗，失败静默（手动检查在设置页）。
  api.checkAppUpdate('zh-CN').then((result) => {
    if (result.update && !result.all_failed) {
      updateInfo.value = result.update
      showUpdate.value = true
    }
  }).catch(() => undefined)
})
</script>

<template>
  <n-config-provider abstract :theme="darkTheme" :theme-overrides="themeOverrides" :locale="zhCN"
    :date-locale="dateZhCN">
    <n-global-style />
    <n-message-provider placement="bottom-right">
      <n-dialog-provider>
        <div class="app-shell">
          <AppSidebar :version="version" />
          <main class="app-main">
            <header class="page-header">
              <h1>{{ $route.meta.title }}</h1>
            </header>
            <DownloadStrip />
            <div class="page-scroll">
              <div class="page-body">
                <router-view />
              </div>
            </div>
          </main>
        </div>
        <UpdatePrompt v-model:show="showUpdate" :update="updateInfo" />
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<style scoped>
.page-header {
  padding: 22px 28px 14px;
  border-bottom: 1px solid #232a33;
}
.page-header h1 {
  margin: 0;
  font-size: 24px;
  font-weight: 650;
  letter-spacing: -0.01em;
}
</style>
