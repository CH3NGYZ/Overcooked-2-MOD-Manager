<script setup lang="ts">
import { NConfigProvider, NDialogProvider, NMessageProvider, NScrollbar } from 'naive-ui'
import { getVersion } from '@tauri-apps/api/app'
import { computed, onMounted, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { CloudUploadOutline } from '@vicons/ionicons5'
import AppSidebar from './components/AppSidebar.vue'
import AppTitleBar from './components/AppTitleBar.vue'
import DownloadStrip from './components/DownloadStrip.vue'
import UpdatePrompt from './components/UpdatePrompt.vue'
import TutorialOverlay from './components/TutorialOverlay.vue'
import { acceptLabel } from './dropAccept'
import { dragActive, dropAcceptTypes } from './composables/useFileDrop'
import { startGameWatch } from './composables/useDashboardStore'
import { requestMapSourcesStartupRefresh } from './composables/useMapSources'
import { useRepositoryRefresh } from './composables/useRepositoryRefresh'
import { endNavigation, expectPageScroll, pageAnimation } from './pageTransition'
import { pageTitleOverride, resetPageHeader } from './pageHeader'
import { api } from './services/tauri'
import { isTauri } from './runtime'
import type { AppUpdateInfo, PublicConfig } from './types'
import { naiveTheme, themeOverrides } from './theme'
import { naiveDateLocale, naiveLocale, t } from './i18n'
import { setTutorialProgressSink, onTutorialSettled, startTutorialIfFirstRun } from './tutorial/engine'

const version = ref('')
const updateInfo = ref<AppUpdateInfo | null>(null)
const showUpdate = ref(false)

/** 页头标题：阶段覆盖值优先，否则用路由 meta；两者都过翻译表。 */
const route = useRoute()
const resolvedTitle = computed(() => t(pageTitleOverride.value ?? (route.meta.title as string) ?? ''))

/**
 * 整窗拖放提示条。
 *
 * Tauri 接管 WebView2 的拖放后，系统光标固定是「禁止」（UIPI 之外还有
 * DROPEFFECT_NONE），CSS 改不了 —— 光看光标分不清这页到底能不能放文件。
 * 所以在拖拽悬停期间给一条明确提示：这页接受什么、松手会发生什么。
 *
 * 没有页面注册拖放（总览 / 改名 / 工具 / 设置）时 `dropAcceptTypes` 为空，
 * 提示不出现：那几页确实没有可导入的东西，提示反而是误导。
 */
const showDropHint = computed(() => dragActive.value && dropAcceptTypes.value.length > 0)
const dropTypesLabel = computed(() => acceptLabel(dropAcceptTypes.value))

/**
 * 安装器启动后的首次仓库刷新：与手动整源刷新走同一引擎。
 * 放在 App 层而不是 MOD 安装页，切换页面不会重复触发。
 *
 * 门控在下面的 `onMounted` 里：先注册 `onTutorialSettled`，再决定是否启动
 * 教程，刷新总在教程之后发生，不会在遮罩底下转圈。
 */
const repositoryRefresh = useRepositoryRefresh()

/* ===== 页面切换动画 ===== */
/* 页面滚动容器：n-scrollbar 实例 ref（scrollTo(x, y)，用于切页复位） */
const pageScrollRef = ref<{ scrollTo: (x: number, y: number) => void } | null>(null)

/**
 * 侧栏导航顺序：决定页面切换时向上还是向下滑动。
 *
 * 必须与 `AppSidebar` 的 sections 顺序一致（总览 → mod 区 → 地图区 → 工具），
 * 并且**列全**每个路由名：漏掉的页面会让 `indexOf` 返回 -1，切过去时既不设
 * 转场也不复位滚动（地图两页曾长期因此不走转场动画）。
 */
const NAV_ORDER = ['overview', 'mods-online', 'mods-local', 'maps-online', 'levels', 'rename', 'tools', 'settings']

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
  /*
   * 教程进度写回 config.toml：引擎只负责「哪些步骤看过了」，落盘在这里接。
   * 写失败不影响播放 —— 进度是新版教程的体验优化，不是启动前提。
   */
  setTutorialProgressSink((ids) => {
    api.saveSettings({ tutorial_watched_steps: ids }).catch(() => undefined)
  })
  /*
   * 启动刷新推迟到教程结束（讲完或跳过）：教程期间满屏刷新转圈会盖住
   * 要讲的卡片，用户读卡片时数据还在跳。没在教程里时 onTutorialSettled
   * 立刻回调，行为与没有教程时一致。
   *
   * 注意顺序：必须先注册这个回调、再启动教程。否则 `api.config()` 还没
   * 回来时 onTutorialSettled 会因为「当前没在教程里」直接放行刷新。
   */
  const settleRefresh = () => {
    onTutorialSettled(() => {
      repositoryRefresh.requestStartupRefresh()
      /*
       * 地图源也要在启动时自动读一次：`release-tags` 源（一个仓库放多张地图）的
       * 地图只能联网读 Release note 才知道，内置快照里没有。只读源树、不刷
       * 每张图的 Release，与上面 MOD 仓库的启动刷新同一时机、同一层级。
       */
      requestMapSourcesStartupRefresh()
    })
  }
  // 浏览器预览没有 Tauri IPC：跳过版本读取与启动更新检查，避免控制台噪音。
  if (!isTauri()) {
    startTutorialIfFirstRun()
    settleRefresh()
    return
  }
  getVersion().then((value) => { version.value = value }).catch(() => undefined)
  void (async () => {
    /*
     * 一次配置读取服务两件事：教程进度（只播没看过的步骤）与
     * 「启动时检查程序更新」开关（关闭后不能再发起 check_app_update，
     * 否则 F5 重载时 onMounted 重跑仍会弹窗）。
     *
     * 读不到配置也要让教程按「都没看过」跑：不能因为一次 IPC 失败，
     * 让首启用户永远看不到教程。失败静默，手动检查在设置页。
     */
    let config: PublicConfig | null = null
    try {
      config = await api.config()
    } catch {
      // 按首次启动处理
    }
    startTutorialIfFirstRun(config?.tutorial_watched_steps ?? [])
    settleRefresh()
    if (!config?.network.check_updates_on_start) return
    try {
      const result = await api.checkAppUpdate('zh-CN')
      if (result.update && !result.all_failed) {
        updateInfo.value = result.update
        showUpdate.value = true
      }
    } catch {
      // 启动更新检查失败静默：手动检查在设置页
    }
  })()
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
            <main class="app-main">              <header class="page-header">
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
          <!--
            整窗拖放提示条：`position: fixed` + 不接收指针事件，不参与布局也不挡住
            页面上的任何东西（来源卡片打开时拖放区仍然能被点亮，它才是高亮对象）。
          -->
          <Transition name="drop-hint">
            <div v-if="showDropHint" class="drop-hint" role="status" aria-live="polite">
              <CloudUploadOutline width="16" height="16" class="drop-hint-icon" />
              <span>{{ t('tools.releaseToInstall', { types: dropTypesLabel }) }}</span>
            </div>
          </Transition>
        </div>
        <UpdatePrompt v-model:show="showUpdate" :update="updateInfo" />
        <!-- 内置教程覆盖层：Teleport 到 body，自己接管点击/滚轮/方向键 -->
        <TutorialOverlay />
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

/*
 * 整窗拖放提示条。
 *
 * 定位基准是**内容区**而不是窗口：水平居中于「右侧不含侧栏」的那块，垂直放
 * 在中下（`top: 62%`）。贴窗口底部居中时它会横跨侧栏与内容区，既不对齐任何
 * 东西，也和右下角的浮动按钮抢同一片视线；页头那一带是各页自己的操作区
 * （批量开关 / 检测更新 / 打开目录），摆上去会被当成又一个按钮。
 *
 * `pointer-events: none` 是硬要求 —— 拖拽期间任何挡住命中测试的元素都会让
 * 「松开发生在提示条上」变成松开在提示条上而不是页面。
 */
.drop-hint {
  position: fixed;
  left: calc(var(--oc2-rail-width) + (100vw - var(--oc2-rail-width)) / 2);
  top: 62%;
  transform: translate(-50%, -50%);
  z-index: var(--oc2-layer-drop);
  display: flex;
  align-items: center;
  gap: 8px;
  max-width: min(420px, 80vw);
  padding: 8px 14px;
  border: 1px solid color-mix(in srgb, var(--oc2-accent) 75%, transparent);
  border-radius: 999px;
  /*
   * 毛玻璃/亚克力。三层叠加，缺一层就退化成「半透明色块」：
   *   · 底色取主题浮层那一档再降到七成（`--oc2-popover`，换主题色与明暗时一起变）；
   *   · `backdrop-filter` 把背后的卡片糊掉，文字仍然压得住；
   *   · `inset` 高光在上边缘留一道极淡亮线，这是亚克力与单纯半透明的主要区别。
   * 旧值 `--oc2-surface` 全仓从未定义过，提示条其实一直没有底色（纯透明）。
   */
  background: color-mix(in srgb, var(--oc2-popover) 72%, transparent);
  -webkit-backdrop-filter: blur(16px) saturate(160%);
  backdrop-filter: blur(16px) saturate(160%);
  box-shadow:
    0 10px 28px color-mix(in srgb, var(--oc2-body) 55%, transparent),
    inset 0 1px 0 color-mix(in srgb, var(--oc2-text-1) 10%, transparent);
  color: var(--oc2-text-1);
  font-size: 12.5px;
  line-height: 1.3;
  pointer-events: none;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.drop-hint-icon { flex: 0 0 auto; color: var(--oc2-accent); }
.drop-hint-enter-active,
.drop-hint-leave-active {
  transition: opacity .12s ease, transform .12s ease;
}
.drop-hint-enter-from,
.drop-hint-leave-to { opacity: 0; transform: translate(-50%, calc(-50% + 6px)); }
@media (prefers-reduced-motion: reduce) {
  .drop-hint-enter-active,
  .drop-hint-leave-active { transition: none; }
}
</style>
