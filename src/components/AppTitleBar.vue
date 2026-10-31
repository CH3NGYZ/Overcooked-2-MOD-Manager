<script setup lang="ts">
import { computed, ref } from 'vue'
import { NDropdown, NPopover, NSpin } from 'naive-ui'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  CloseOutline, ContrastOutline, ContractOutline, LanguageOutline, MoonOutline, PlayOutline,
  RemoveOutline, SquareOutline, StopOutline, SunnyOutline,
} from '@vicons/ionicons5'
import { ACCENTS, accentKey, cycleAccent, isDark, setAccent, setThemeMode, themeMode } from '../theme'
import { useNotify } from '../useNotify'
import { currentLocaleLabel, locale, localeOptions, setLocale, t, type Locale } from '../i18n'
import { useUiZoom } from '../composables/useUiZoom'
import { useDashboard } from '../composables/useDashboardStore'
import { api } from '../services/tauri'
import { isTauri, safeTauri, safeTauriSync } from '../runtime'
import { isMacWindow } from '../platform'
import appIcon from '../assets/app-icon.png'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'

const props = defineProps<{ version: string }>()

/**
 * 标题栏三个弹层（缩放 / 语言 / 主题色）的层级。
 *
 * 必须高于页面上所有其它浮层 —— 包括下载遮罩（2600）与全屏阻塞遮罩
 * （4000）。这三个是挂在自绘标题栏上的真窗口级控件，被下载卡片盖住时
 * 用户会以为按钮失效。取值与 `global.css` 的 --oc2-layer-titlebar-menu 一致。
 */
const TITLEBAR_MENU_Z_INDEX = 5000

/**
 * 窗口对象只在真实 Tauri 环境存在。浏览器预览下必须为 null，
 * 否则 getCurrentWindow() 抛错会让 setup 崩溃、整页白屏。
 */
const appWindow = safeTauriSync(() => getCurrentWindow()) ?? null
const maximized = ref(false)
const { zoomPercent, setZoom } = useUiZoom()

void safeTauri(async () => {
  const window = getCurrentWindow()
  maximized.value = await window.isMaximized()
})
let unlistenResize: (() => void) | null = null
void safeTauri(async () => {
  const window = getCurrentWindow()
  const unlisten = await window.onResized(async () => {
    maximized.value = await window.isMaximized().catch(() => false)
  })
  unlistenResize = unlisten
})

function onUnmountedCleanup() { unlistenResize?.() }

/** 九色的显示名（仅用于 hover 提示与无障碍，矩阵本身不显示文字）。 */
const ACCENT_NAMES: Record<string, string> = {
  slate: 'nav.slate', gray: 'nav.gray', rose: 'nav.rose',
  red: 'nav.red', orange: 'nav.orange', amber: 'nav.amber',
  green: 'nav.green', teal: 'nav.teal', blue: 'nav.blue',
}
const accentLabel = (key: string) => t(ACCENT_NAMES[key] ?? key)
/** 当前主题色块（触发器按钮用），矩阵里直接读 ACCENTS。 */
const accentSwatch = computed(() => ACCENTS.find((entry) => entry.key === accentKey.value)?.swatch ?? '#14b8a6')

const themeLabel = computed(() => (isDark.value ? t('nav.dark') : t('nav.light')))

// 窗口控制：浏览器预览下按钮仍可见，但调用被安全吞掉（不抛错）。
async function minimize() { await safeTauri(async () => appWindow?.minimize()) }
async function toggleMaximize() {
  await safeTauri(async () => {
    if (!appWindow) return
    await appWindow.toggleMaximize()
    maximized.value = await appWindow.isMaximized().catch(() => false)
  })
}
async function close() { await safeTauri(async () => appWindow?.close()) }
/** 浏览器预览不显示窗口控制（无窗口可控制）。 */
const canControlWindow = computed(() => isTauri())
/**
 * macOS 的窗控在左侧、顺序是 关闭 / 最小化 / 最大化（绿点等于「缩放/全屏」）。
 * 与 Windows 右侧的 最小化 / 最大化 / 关闭 相反，位置和顺序都要换。
 */
const macChrome = isMacWindow()

/* ===== 厨房启动 / 停止（悬浮在应用图标上） ===== */

const message = useNotify()
const { game, recheckGameState } = useDashboard()
/** 启动/停止进行中，按钮显示 loading 并禁止重复点击。 */
const launching = ref(false)
/** 悬浮提示是否展开。 */
const gameHover = ref(false)

/** 游戏是否在运行：来自后端游戏状态观察器，前端不自己探测。 */
const kitchenRunning = computed(() => !!game.value?.kitchen_running)
/** 有游戏目录才允许启动。 */
const canLaunch = computed(() => !!game.value?.path)

/**
 * 切换厨房运行状态。
 *
 * 启动与停止都走 Steam 语义：
 *   - 启动：后端 `launch_normal` → `steam://rungameid/728880`（Steam 负责拉起游戏）；
 *   - 停止：Steam 没有「停止」协议，后端 `stop_game` 结束游戏进程。
 *
 * 命令返回后不靠「猜几秒」：`stop_game` 内部已经重算快照并推送事件，
 * 这里再主动重算一次让按钮当场摆正；启动时进程要几秒才出现，所以补几次
 * 重算（每次只重算游戏状态，不重扫 MOD），直到按钮真的翻转就停。
 * 之所以不建议在这里轮询 `load()`：那会把整份 Dashboard 连同 MOD 扫描
 * 一起重做，几次下去比状态本身贵得多。
 */
async function toggleGame() {
  if (launching.value) return
  launching.value = true
  const wasRunning = kitchenRunning.value
  try {
    const result = wasRunning ? await api.stopGame() : await api.launchNormal()
    result.ok ? message.success(translateBackendMessage(result.message)) : message.error(translateBackendMessage(result.message))
    if (result.ok) {
      for (let attempt = 0; attempt < 6; attempt += 1) {
        await recheckGameState()
        if (kitchenRunning.value !== wasRunning) break
        await new Promise((resolve) => window.setTimeout(resolve, 700))
      }
    }
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    launching.value = false
  }
}

// 缩放：下拉选择（与语言下拉同款），25% 步进，100%–200%，默认 150%
const ZOOM_STEPS = [100, 125, 150, 175, 200] as const
const zoomOptions = ZOOM_STEPS.map((step) => ({ label: `${step}%`, key: step }))
const zoomKey = computed(() => ZOOM_STEPS.find((step) => step === zoomPercent.value) ?? 150)
function onZoomSelect(key: number) {
  void setZoom(key / 100)
}

defineExpose({ onUnmountedCleanup })
</script>

<template>
  <header class="titlebar" data-tauri-drag-region>
    <!--
      macOS：窗控在最左侧，顺序按系统习惯 关闭 / 最小化 / 最大化。
      三个圆点的图标默认隐藏、整组悬停才出现，颜色语义与系统一致
      （红=关闭、黄=最小化、绿=缩放）。Windows 仍是右侧的方角按钮。
    -->
    <template v-if="canControlWindow && macChrome">
      <div class="traffic">
        <button class="dot close" type="button" :title="t('nav.close')" :aria-label="t('nav.close')" @click="close">
          <CloseOutline width="9" height="9" />
        </button>
        <button class="dot min" type="button" :title="t('nav.minimize')" :aria-label="t('nav.minimize')" @click="minimize">
          <RemoveOutline width="9" height="9" />
        </button>
        <button class="dot max" type="button" :title="maximized ? t('nav.restore') : t('nav.maximize')"
          :aria-label="maximized ? t('nav.restore') : t('nav.maximize')" @click="toggleMaximize">
          <ContractOutline v-if="maximized" width="9" height="9" />
          <SquareOutline v-else width="8" height="8" />
        </button>
      </div>
    </template>

    <!-- 品牌：图标上悬浮启动/停止按钮 -->
    <div class="brand">
      <!--
        brand-mark-wrap 不能带 data-tauri-drag-region：
        Tauri 的拖拽区域会在 mousedown 时接管事件，子元素按钮就点不动了。
      -->
      <div class="brand-mark-wrap" @mouseenter="gameHover = true" @mouseleave="gameHover = false">
        <img class="brand-mark" :src="appIcon" alt="" />
        <!--
          悬浮在应用图标上的启动/停止按钮。
          运行中显示「停止」（红色），未运行显示「启动」；
          浏览器预览没有 IPC，仍然渲染但点击无副作用。
        -->
        <button class="brand-game-toggle" type="button"
          :class="{ running: kitchenRunning }"
          :disabled="launching || !canLaunch"
          :title="kitchenRunning ? t('overview.stopGame') : t('overview.launchGame')"
          @click.stop="toggleGame">
          <n-spin v-if="launching" :size="14" />
          <StopOutline v-else-if="kitchenRunning" width="15" height="15" />
          <PlayOutline v-else width="15" height="15" />
        </button>
        <span v-if="gameHover" class="brand-game-hint">
          {{ kitchenRunning ? t('overview.stopGame') : t('overview.launchGame') }}
        </span>
      </div>
      <div class="brand-text" data-tauri-drag-region>
        <div class="brand-title-row">
          <strong>{{ t('nav.oc2ModManager') }}</strong>
          <span v-if="props.version" class="brand-version">v{{ props.version }}</span>
        </div>
        <small>{{ t('nav.quicklyManageModsForOvercooked2') }}</small>
      </div>
    </div>

    <div class="spacer" data-tauri-drag-region></div>

    <!-- 界面缩放：下拉选择（与语言下拉一致） -->
    <n-dropdown :options="zoomOptions" :value="zoomKey" trigger="click" :z-index="TITLEBAR_MENU_Z_INDEX"
      @select="onZoomSelect">
      <button class="pill" type="button" :title="t('nav.uiScale')">
        <ContrastOutline width="15" height="15" />
        <span>{{ zoomPercent }}%</span>
        <span class="caret">▾</span>
      </button>
    </n-dropdown>

    <!-- 语言 -->
    <n-dropdown :options="localeOptions.map(o => ({ label: o.label, key: o.value }))" :value="locale"
      trigger="click" :z-index="TITLEBAR_MENU_Z_INDEX"
      @select="(key: Locale) => setLocale(key)">
      <button class="pill" type="button" :title="t('nav.language')">
        <LanguageOutline width="15" height="15" />
        <span>{{ currentLocaleLabel }}</span>
        <span class="caret">▾</span>
      </button>
    </n-dropdown>

    <!-- 主题模式 -->
    <button class="pill icon-only" type="button" :title="themeLabel"
      @click="setThemeMode(themeMode === 'dark' ? 'light' : 'dark')">
      <MoonOutline v-if="isDark" width="16" height="16" />
      <SunnyOutline v-else width="16" height="16" />
    </button>

    <!-- 主题色：3×3 色块矩阵，只显示颜色 -->
    <n-popover trigger="click" placement="bottom-end" :show-arrow="false" raw
      :z-index="TITLEBAR_MENU_Z_INDEX">
      <template #trigger>
        <button class="pill icon-only" type="button" :title="t('nav.accent')" @contextmenu.prevent="cycleAccent">
          <span class="swatch" :style="{ background: accentSwatch }"></span>
        </button>
      </template>
      <div class="accent-grid" role="listbox" :aria-label="t('nav.accent')">
        <button v-for="entry in ACCENTS" :key="entry.key" type="button" class="accent-cell"
          :class="{ active: entry.key === accentKey }" role="option" :aria-selected="entry.key === accentKey"
          :title="accentLabel(entry.key)" :style="{ background: entry.swatch }" @click="setAccent(entry.key)" />
      </div>
    </n-popover>

    <!-- 窗口控制：仅真实 Tauri 窗口显示；Windows 侧保持右侧的方角按钮 -->
    <template v-if="canControlWindow && !macChrome">
      <span class="sep"></span>

      <button class="win-btn" type="button" :title="t('nav.minimize')" :aria-label="t('nav.minimize')" @click="minimize">
        <RemoveOutline width="16" height="16" />
      </button>
      <button class="win-btn" type="button" :title="maximized ? t('nav.restore') : t('nav.maximize')"
        :aria-label="maximized ? t('nav.restore') : t('nav.maximize')" @click="toggleMaximize">
        <ContractOutline v-if="maximized" width="15" height="15" />
        <SquareOutline v-else width="14" height="14" />
      </button>
      <button class="win-btn danger" type="button" :title="t('nav.close')" :aria-label="t('nav.close')" @click="close">
        <CloseOutline width="17" height="17" />
      </button>
    </template>
  </header>
</template>

<style scoped>
.titlebar {
  flex: 0 0 auto;
  height: var(--oc2-titlebar-height, 56px);
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 8px 0 14px;
  background: var(--oc2-rail);
  border-bottom: 1px solid var(--oc2-rail-line);
  user-select: none;
  /*
   * 标题栏不参与 z-index 竞争：全屏遮罩在 global.css 里让出了它的高度，
   * 所以它天然露在最上方，拖动区域不会被遮罩吞掉。
   * 这里只建立层叠上下文，防止内容区的子元素意外盖上来。
   */
  position: relative;
  z-index: 1;
}
.brand {
  display: flex;
  align-items: center;
  gap: 11px;
  /* 品牌区可收缩：窗口窄时优先让标题文字省略，而不是挤掉右侧控件 */
  flex: 0 1 auto;
  min-width: 0;
}
.brand-mark-wrap {
  position: relative;
  flex: 0 0 34px;
  width: 34px;
  height: 34px;
  /* 悬浮按钮会超出图标边界，不能裁切；提示文字也用这块定位上下文 */
}
.brand-mark {
  width: 34px;
  height: 34px;
  flex: 0 0 34px;
  border-radius: 9px;
  object-fit: cover;
  display: block;
  transition: filter .16s ease, transform .16s ease;
}
/* 悬浮时压暗图标，让按钮更清晰 */
.brand-mark-wrap:hover .brand-mark { filter: brightness(.55); }
/*
 * 启动/停止按钮：覆盖在应用图标中央。
 *
 * 图标本身是彩色插画，浅色底纹几乎看不出来，所以这里用「不透明白色圆形
 * + 深色图标」的高对比方案，不依赖悬浮才可见 —— 用户第一眼就该知道
 * 这里能点。运行中切成实心红色圆形 + 白色停止图标。
 */
.brand-game-toggle {
  position: absolute;
  inset: 0;
  display: grid;
  place-items: center;
  padding: 0;
  border: 0;
  border-radius: 9px;
  background: transparent;
  color: #fff;
  cursor: pointer;
  transition: background-color .16s ease;
}
/* 圆形底盘：让图标在任意插画底色上都清晰 */
.brand-game-toggle::before {
  content: '';
  position: absolute;
  width: 22px;
  height: 22px;
  border-radius: 50%;
  background: rgba(255, 255, 255, .92);
  box-shadow: 0 1px 4px rgba(0, 0, 0, .45);
  transition: background-color .16s ease, transform .16s ease;
}
.brand-game-toggle > svg {
  position: relative;
  color: #11151a;
}
.brand-mark-wrap:hover .brand-game-toggle::before { transform: scale(1.08); }
/* 运行中：红色底盘 + 白色图标，和「正在运行」语义一致 */
.brand-game-toggle.running::before { background: #ef4444; }
.brand-game-toggle.running { color: #fff; }
.brand-game-toggle.running > svg { color: #fff; }
.brand-game-toggle:disabled { cursor: not-allowed; opacity: .5; }
/* 悬浮提示：图标下方的一行小标签 */
.brand-game-hint {
  position: absolute;
  top: calc(100% + 6px);
  left: 50%;
  transform: translateX(-50%);
  padding: 3px 8px;
  border-radius: 6px;
  background: var(--oc2-popover);
  border: 1px solid var(--oc2-row-line);
  box-shadow: 0 6px 18px rgba(0, 0, 0, .35);
  font-size: 11px;
  line-height: 1.4;
  color: var(--oc2-text-1);
  white-space: nowrap;
  pointer-events: none;
  z-index: 10;
}
.brand-text { display: flex; flex-direction: column; min-width: 0; }
.brand-title-row { display: flex; align-items: baseline; gap: 6px; min-width: 0; }
/* 版本号跟在标题后面，弱化到副标题层级，不抢标题的视觉权重。 */
.brand-version {
  font-size: 11px;
  font-weight: 500;
  color: var(--oc2-text-3);
  letter-spacing: .2px;
  white-space: nowrap;
}
.brand-text strong,
.brand-text small {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.brand-text strong { font-size: 14.5px; font-weight: 700; color: var(--oc2-text-1); }
.brand-text small { font-size: 11px; color: var(--oc2-text-3); margin-top: 1px; }
.spacer { flex: 1 1 auto; height: 100%; min-width: 8px; }
.pill {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  height: 32px;
  padding: 0 11px;
  border-radius: 9px;
  border: 1px solid var(--oc2-rail-line);
  background: var(--oc2-card);
  color: var(--oc2-text-2);
  font-size: 12.5px;
  cursor: pointer;
  transition: border-color .16s ease, color .16s ease, background-color .16s ease;
  /*
   * 空间不足时按钮自身不收缩、文字不换行：
   * 图标与箭头保持可见，中间的文本按需截断（1 行 + 省略号）。
   */
  flex: 0 0 auto;
  white-space: nowrap;
  min-width: 0;
  max-width: 160px;
}
/* 文本段允许收缩并省略，图标与箭头固定不缩 */
.pill > span:not(.caret):not(.swatch) {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.pill > svg,
.pill > .caret,
.pill > .swatch { flex: 0 0 auto; }
.pill:hover { border-color: var(--oc2-accent); color: var(--oc2-text-1); }
.pill.icon-only { padding: 0; width: 32px; max-width: 32px; justify-content: center; }
.caret { color: var(--oc2-text-4); font-size: 10px; }
.swatch {
  width: 15px; height: 15px; border-radius: 50%;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, .25) inset;
}
.sep {
  width: 1px; height: 22px; margin: 0 2px;
  background: var(--oc2-rail-line);
}

/*
 * macOS 窗控：三个 13px 圆点，颜色语义与系统一致。
 * 图标默认透明，整组悬停才浮现 —— 系统也是这个行为，且能避免在深色
 * 标题栏上出现三团深色噪点。圆点之间 8px，符合 macOS 的间距习惯。
 */
.traffic {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
  /* 与右侧按钮区对称：给标题栏左缘留一点呼吸 */
  padding: 0 6px 0 0;
}
.dot {
  width: 13px;
  height: 13px;
  padding: 0;
  border: 0;
  border-radius: 50%;
  display: grid;
  place-items: center;
  cursor: pointer;
  color: rgba(0, 0, 0, .58);
  transition: filter .14s ease;
}
.dot > svg { opacity: 0; transition: opacity .12s ease; }
.traffic:hover .dot > svg { opacity: 1; }
.dot.close { background: #ff5f57; }
.dot.min { background: #febc2e; }
.dot.max { background: #28c840; }
.dot:active { filter: brightness(.86); }
.dot:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }

.win-btn {
  width: 40px;
  height: 32px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 8px;
  background: transparent;
  color: var(--oc2-text-2);
  cursor: pointer;
  transition: background-color .14s ease, color .14s ease;
}
.win-btn:hover { background: var(--oc2-row-hover); color: var(--oc2-text-1); }
.win-btn.danger:hover { background: #d64550; color: #fff; }
</style>
