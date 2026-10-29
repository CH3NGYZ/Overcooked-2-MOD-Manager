<script setup lang="ts">
import { NConfigProvider, NDialogProvider, NMessageProvider, NScrollbar } from 'naive-ui'
import { getVersion } from '@tauri-apps/api/app'
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import AppSidebar from './components/AppSidebar.vue'
import AppTitleBar from './components/AppTitleBar.vue'
import DownloadStrip from './components/DownloadStrip.vue'
import UpdatePrompt from './components/UpdatePrompt.vue'
import { startGameWatch } from './composables/useDashboardStore'
import { useRepositoryRefresh } from './composables/useRepositoryRefresh'
import { endNavigation, expectPageScroll, pageAnimation } from './pageTransition'
import { pageTitleOverride, resetPageHeader } from './pageHeader'
import { api } from './services/tauri'
import { isTauri } from './runtime'
import type { AppUpdateInfo } from './types'
import { naiveTheme, themeOverrides } from './theme'
import { naiveDateLocale, naiveLocale, t } from './i18n'

const version = ref('')
const updateInfo = ref<AppUpdateInfo | null>(null)
const showUpdate = ref(false)

/** 页头标题：阶段覆盖值优先，否则用路由 meta；两者都过翻译表。 */
const route = useRoute()
const resolvedTitle = computed(() => t(pageTitleOverride.value ?? (route.meta.title as string) ?? ''))

/**
 * 安装器启动后的首次仓库刷新：与手动整源刷新走同一引擎。
 * 放在 App 层而不是 MOD 安装页，切换页面不会重复触发。
 * 教程系统接入后：入口处先 `deferStartupRefresh()`，教程结束再 `releaseStartupRefresh()`。
 */
const repositoryRefresh = useRepositoryRefresh()

/* ===== 页面切换动画 ===== */
/* 页面滚动容器：n-scrollbar 实例 ref（scrollTo(x, y)，用于切页复位） */
const pageScrollRef = ref<{ scrollTo: (x: number, y: number) => void } | null>(null)

/** 侧栏导航顺序：决定页面切换时向上还是向下滑动。 */
const NAV_ORDER = ['overview', 'framework', 'mods-online', 'mods-local', 'rename', 'tools', 'settings']

/**
 * 越靠下点击 → 新页面从下方滑入（page-down）；越靠上点击 → 从上方滑入（page-up）。
 * 带 `focus` 查询的跳转需要滚动到指定卡片，那时改用淡入（page-fade）：
 * 位移和滚动是两个平滑动作，同时跑会互相打架。
 */
let lastNavIndex = NAV_ORDER.indexOf(String(route.name))
/** 本次切页是否要在新页面挂载时把滚动位置归零（带 focus 目标时为 false）。 */
let resetScrollOnEnter = false

watch(
  () => route.name,
  (name) => {
    // 标题覆盖只在当前页面阶段有效：任何路由切换都先清掉
    resetPageHeader()
    const index = NAV_ORDER.indexOf(String(name))
    if (index === -1) return
    const hasFocusTarget = !!route.query.focus
    pageAnimation.value = hasFocusTarget ? 'page-fade' : index >= lastNavIndex ? 'page-down' : 'page-up'
    lastNavIndex = index
    // 带定位目标时交给页面自身滚动，这里不归零，否则会先跳到顶部再滑到卡片
    expectPageScroll(hasFocusTarget)
    /*
     * 归零必须推迟到新页面挂载前一刻（Transition 的 before-enter），
     * 不能在路由 watch 里立刻执行：out-in 过渡会先保留旧页面淡出，
     * 此刻滚到顶部会让用户先看到旧页面顶部一闪，然后才进入切换动画。
     */
    resetScrollOnEnter = !hasFocusTarget
  },
)

/** 新页面插入 DOM、尚未绘制时复位滚动位置。 */
function onPageBeforeEnter() {
  if (!resetScrollOnEnter) return
  resetScrollOnEnter = false
  pageScrollRef.value?.scrollTo(0, 0)
}

onMounted(() => {
  startGameWatch()
  // 浏览器预览没有 Tauri IPC：跳过版本读取与启动更新检查，避免控制台噪音。
  if (!isTauri()) return
  getVersion().then((value) => { version.value = value }).catch(() => undefined)
  /*
   * 启动更新检查必须先读配置：设置里的「启动时检查程序更新」关闭后
   * 不能再发起 check_app_update，否则 F5 重载（onMounted 重跑）仍会弹窗。
   * 配置读取失败按关闭处理，失败静默（手动检查在设置页）。
   */
  api.config()
    .then((config) => {
      if (!config.network.check_updates_on_start) return
      return api.checkAppUpdate('zh-CN').then((result) => {
        if (result.update && !result.all_failed) {
          updateInfo.value = result.update
          showUpdate.value = true
        }
      })
    })
    .catch(() => undefined)
  repositoryRefresh.requestStartupRefresh()
})
</script>

<template>
  <n-config-provider abstract :theme="naiveTheme" :theme-overrides="themeOverrides" :locale="naiveLocale"
    :date-locale="naiveDateLocale">
    <!--
      不使用 n-global-style：body 的底色/文字/字体/行高由 global.css 直接托管，
      并且它会往 document.body 写样式与 n-styled 标记 —— HMR 重挂载时
      旧实例的清理与新实例的初始化会叠加，devtools 就报
      「More than one n-global-style exist in the document.body」。
      减少这一层还避免了它每次主题变化时重写 body 样式。
    -->
    <n-message-provider placement="bottom-left">
      <n-dialog-provider>
        <div class="app-shell">
          <AppTitleBar :version="version" />
          <div class="app-body">
            <AppSidebar />
            <main class="app-main">
              <header class="page-header">
                <div class="page-header-title">
                  <!--
                    标题固定在这里，不参与页面的滑动过渡。
                    切页与阶段变化（如改多彩名 → 开始自定义）只做淡入淡出，
                    避免整行标题随页面上下移动。
                  -->
                  <Transition name="title-fade" mode="out-in">
                    <h1 :key="resolvedTitle">{{ resolvedTitle }}</h1>
                  </Transition>
                  <div id="page-header-title-extra" class="page-header-title-extra"></div>
                </div>
                <div id="page-header-actions" class="page-header-actions"></div>
              </header>
              <DownloadStrip />
              <!-- 内容滚动用 Naive 的 n-scrollbar：滚动条悬浮在内容上方，不占布局宽度 -->
              <n-scrollbar ref="pageScrollRef" class="page-scroll">
                <!-- 导航切换：页面内容按方向滑动；带定位目标时改为淡入并由页面自己滚动 -->
                <Transition :name="pageAnimation" mode="out-in" @before-enter="onPageBeforeEnter"
                  @after-enter="endNavigation">
                  <div class="page-body" :key="String($route.name)">
                    <router-view />
                  </div>
                </Transition>
              </n-scrollbar>
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
/*
 * 页头布局规则放在 global.css（.page-header / .page-header-title /
 * .page-header-actions），因为各页面通过 Teleport 往页头塞内容，
 * 这些是跨组件的结构约定，不属于 App.vue 的 scoped 样式。
 */

/*
 * 标题淡入淡出：页头本身固定不动，只有 h1 的文字在切换。
 * 用于两类变化 —— 路由切换（meta.title 变）和页面阶段变化
 * （如改多彩名 → 开始自定义）。out-in 保证两段文字不重叠。
 */
.title-fade-enter-active,
.title-fade-leave-active {
  transition: opacity .16s ease;
}
.title-fade-enter-from,
.title-fade-leave-to { opacity: 0; }
@media (prefers-reduced-motion: reduce) {
  .title-fade-enter-active,
  .title-fade-leave-active { transition: none; }
}

/*
 * 导航切换动画。
 *
 * 只用 opacity + 极小的位移（6px）。位移再大时，元素会移出滚动容器的
 * 可视区而被 n-scrollbar 的 overflow: hidden 裁掉，表现为「动画末期
 * 底部卡片缺一条、最后一帧才还原」。6px 既保留了方向感，
 * 又不会让内容越过滚动容器边界。
 *
 * 另加 will-change: transform, opacity 让浏览器提前提层，
 * 避免动画开始时才合成导致首帧跳变。
 */
.page-down-enter-active,
.page-down-leave-active,
.page-up-enter-active,
.page-up-leave-active,
.page-fade-enter-active,
.page-fade-leave-active {
  transition: opacity .18s ease, transform .18s cubic-bezier(.32, .72, .35, 1);
  will-change: transform, opacity;
}
.page-down-enter-from { opacity: 0; transform: translateY(6px); }
.page-down-leave-to { opacity: 0; transform: translateY(-6px); }
.page-up-enter-from { opacity: 0; transform: translateY(-6px); }
.page-up-leave-to { opacity: 0; transform: translateY(6px); }
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
