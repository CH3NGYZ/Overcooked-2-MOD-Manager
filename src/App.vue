<script setup lang="ts">
import { NConfigProvider, NDialogProvider, NMessageProvider, NGlobalStyle, zhCN, dateZhCN } from 'naive-ui'
import { getVersion } from '@tauri-apps/api/app'
import { onMounted, ref } from 'vue'
import AppSidebar from './components/AppSidebar.vue'
import DownloadStrip from './components/DownloadStrip.vue'
import { startGameWatch } from './composables/useDashboardStore'
import { darkTheme, themeOverrides } from './theme'

const version = ref('')
onMounted(() => {
  startGameWatch()
  getVersion().then((value) => { version.value = value }).catch(() => undefined)
})
</script>

<template>
  <n-config-provider :theme="darkTheme" :theme-overrides="themeOverrides" :locale="zhCN" :date-locale="dateZhCN">
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
