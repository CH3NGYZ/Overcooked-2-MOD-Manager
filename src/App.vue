<script setup lang="ts">
import { NConfigProvider, NDialogProvider, NMessageProvider, NGlobalStyle } from 'naive-ui'
import { getVersion } from '@tauri-apps/api/app'
import { nextTick, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import AppSidebar from './components/AppSidebar.vue'
import AppTitleBar from './components/AppTitleBar.vue'
import DownloadStrip from './components/DownloadStrip.vue'
import UpdatePrompt from './components/UpdatePrompt.vue'
import { startGameWatch } from './composables/useDashboardStore'
import { useRepositoryRefresh } from './composables/useRepositoryRefresh'
import { endNavigation, expectPageScroll, pageAnimation } from './pageTransition'
import { api } from './services/tauri'
import { isTauri } from './runtime'
import type { AppUpdateInfo } from './types'
import { naiveTheme, themeOverrides } from './theme'
import { naiveDateLocale, naiveLocale, t } from './i18n'

const version = ref('')
const updateInfo = ref<AppUpdateInfo | null>(null)
const showUpdate = ref(false)

/**
 * 安装器启动后的首次仓库刷新：与手动「刷新全部仓库源」走同一引擎。
 * 放在 App 层而不是 MOD 安装页，切换页面不会重复触发。
 * 教程系统接入后：入口处先 `deferStartupRefresh()`，教程结束再 `releaseStartupRefresh()`。
 */
const repositoryRefresh = useRepositoryRefresh()

/* ===== 页面切换动画 ===== */
const route = useRoute()
const pageScroll = ref<HTMLElement | null>(null)

/** 侧栏导航顺序：决定页面切换时向上还是向下滑动。 */
const NAV_ORDER = ['overview', 'framework', 'mods-online', 'mods-local', 'rename', 'tools', 'settings']

/**
 * 越靠下点击 → 新页面从下方滑入（page-down）；越靠上点击 → 从上方滑入（page-up）。
 * 带 `focus` 查询的跳转需要滚动到指定卡片，那时改用淡入（page-fade）：
 * 位移和滚动是两个平滑动作，同时跑会互相打架。
 */
let lastNavIndex = NAV_ORDER.indexOf(String(route.name))
watch(
  () => route.name,
  (name) => {
    const index = NAV_ORDER.indexOf(String(name))
    if (index === -1) return
    const hasFocusTarget = !!route.query.focus
    pageAnimation.value = hasFocusTarget ? 'page-fade' : index >= lastNavIndex ? 'page-down' : 'page-up'
    lastNavIndex = index
    // 带定位目标时交给页面自身滚动，这里不归零，否则会先跳到顶部再滑到卡片
    expectPageScroll(hasFocusTarget)
    if (hasFocusTarget) return
    void nextTick(() => {
      if (pageScroll.value) pageScroll.value.scrollTop = 0
    })
  },
)

onMounted(() => {
  startGameWatch()
  // 浏览器预览没有 Tauri IPC：跳过版本读取与启动更新检查，避免控制台噪音。
  if (!isTauri()) return
  getVersion().then((value) => { version.value = value }).catch(() => undefined)
  // 启动更新检查：发现新版本才弹窗，失败静默（手动检查在设置页）。
  api.checkAppUpdate('zh-CN').then((result) => {
    if (result.update && !result.all_failed) {
      updateInfo.value = result.update
      showUpdate.value = true
    }
  }).catch(() => undefined)
  repositoryRefresh.requestStartupRefresh()
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
              <div ref="pageScroll" class="page-scroll">
                <!-- 导航切换：页面内容按方向滑动；带定位目标时改为淡入并由页面自己滚动 -->
                <Transition :name="pageAnimation" mode="out-in" @after-enter="endNavigation">
                  <div class="page-body" :key="String($route.name)">
                    <router-view />
                  </div>
                </Transition>
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

/*
 * 导航切换动画：只动 transform / opacity，不触发布局。
 * page-down / page-up 用于普通切页（上下滑动）；
 * page-fade 用于带定位目标的跳转（只淡入，不位移），把滚动留给页面自己。
 */
.page-down-enter-active,
.page-down-leave-active,
.page-up-enter-active,
.page-up-leave-active,
.page-fade-enter-active,
.page-fade-leave-active {
  transition: opacity .18s ease, transform .18s cubic-bezier(.32, .72, .35, 1);
}
.page-down-enter-from { opacity: 0; transform: translateY(14px); }
.page-down-leave-to { opacity: 0; transform: translateY(-10px); }
.page-up-enter-from { opacity: 0; transform: translateY(-14px); }
.page-up-leave-to { opacity: 0; transform: translateY(10px); }
/* 带定位目标：不做位移，避免和「滚动到卡片」同时动 */
.page-fade-enter-from,
.page-fade-leave-to { opacity: 0; }

/* 减少动效偏好：直接切换，不位移 */
@media (prefers-reduced-motion: reduce) {
  .page-down-enter-active,
  .page-down-leave-active,
  .page-up-enter-active,
  .page-up-leave-active,
  .page-fade-enter-active,
  .page-fade-leave-active {
    transition: opacity .12s ease;
  }
  .page-down-enter-from,
  .page-down-leave-to,
  .page-up-enter-from,
  .page-up-leave-to { transform: none; }
}
</style>
