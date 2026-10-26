<script setup lang="ts">
import { NConfigProvider, NDialogProvider, NMessageProvider, NGlobalStyle } from 'naive-ui'
import { getVersion } from '@tauri-apps/api/app'
import { onMounted, ref } from 'vue'
import AppSidebar from './components/AppSidebar.vue'
import AppTitleBar from './components/AppTitleBar.vue'
import DownloadStrip from './components/DownloadStrip.vue'
import UpdatePrompt from './components/UpdatePrompt.vue'
import { startGameWatch } from './composables/useDashboardStore'
import { api } from './services/tauri'
import type { AppUpdateInfo } from './types'
import { naiveTheme, themeOverrides } from './theme'
import { naiveDateLocale, naiveLocale, t } from './i18n'

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
  <n-config-provider abstract :theme="naiveTheme" :theme-overrides="themeOverrides" :locale="naiveLocale"
    :date-locale="naiveDateLocale">
    <n-global-style />
    <n-message-provider placement="bottom-right">
      <n-dialog-provider>
        <div class="app-shell">
          <AppTitleBar :version="version" />
          <div class="app-body">
            <AppSidebar :version="version" />
            <main class="app-main">
              <header class="page-header">
                <div class="page-header-title">
                  <h1>{{ t(($route.meta.title as string) || '') }}</h1>
                  <div id="page-header-title-extra" class="page-header-title-extra"></div>
                </div>
                <div id="page-header-actions" class="page-header-actions"></div>
              </header>
              <DownloadStrip />
              <div class="page-scroll">
                <div class="page-body">
                  <router-view />
                </div>
              </div>
            </main>
          </div>
        </div>
        <UpdatePrompt v-model:show="showUpdate" :update="updateInfo" />
      </n-dialog-provider>
    </n-message-provider>
  </n-config-provider>
</template>

<style scoped>
/* 自绘标题栏在最上方，其下才是「侧栏 + 内容」的横向布局。 */
.app-shell {
  display: flex;
  flex-direction: column;
  width: 100%;
  height: 100%;
  overflow: hidden;
}
.app-body {
  flex: 1 1 0;
  min-height: 0;
  display: flex;
  overflow: hidden;
}
.page-header {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  padding: 16px 28px 12px;
}
.page-header-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  flex: 0 0 auto;
}
.page-header-title {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}
.page-header-title-extra {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}
.page-header h1 {
  margin: 0;
  font-size: 23px;
  font-weight: 650;
  letter-spacing: -0.01em;
  color: var(--oc2-text-1);
  white-space: nowrap;
}
</style>
