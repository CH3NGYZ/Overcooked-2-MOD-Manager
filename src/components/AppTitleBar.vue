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
import appIcon from '../assets/app-icon.png'

const props = defineProps<{ version: string }>()

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
  slate: '板岩', gray: '灰色', rose: '玫红',
  red: '红色', orange: '橙色', amber: '琥珀',
  green: '绿色', teal: '青色', blue: '蓝色',
}
const accentLabel = (key: string) => t(ACCENT_NAMES[key] ?? key)
/** 当前主题色块（触发器按钮用），矩阵里直接读 ACCENTS。 */
const accentSwatch = computed(() => ACCENTS.find((entry) => entry.key === accentKey.value)?.swatch ?? '#14b8a6')

const themeLabel = computed(() => (isDark.value ? t('深色') : t('浅色')))

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

/* ===== 厨房启动 / 停止（悬浮在应用图标上） ===== */

const message = useNotify()
const { game, load } = useDashboard()
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
 * 命令返回后不立即刷新 Dashboard：进程启动/退出需要几秒，
 * 游戏状态由后端观察器每秒推送（`game-state-changed`），
 * 按钮会自动翻转。这里只在若干秒后补一次 `load()`，用于同步 MOD 扫描结果
 * （运行状态导致的可操作项变化），避免用旧快照覆盖刚收到的状态事件。
 */
async function toggleGame() {
  if (launching.value) return
  launching.value = true
  const wasRunning = kitchenRunning.value
  try {
    const result = wasRunning ? await api.stopGame() : await api.launchNormal()
    result.ok ? message.success(result.message) : message.error(result.message)
    if (result.ok) {
      // 轮询几次，让按钮在观察器事件到达前也能尽快反映预期状态
      for (let attempt = 0; attempt < 6; attempt += 1) {
        await new Promise((resolve) => window.setTimeout(resolve, 1000))
        await load()
        if (kitchenRunning.value !== wasRunning) break
      }
    }
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
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
          :title="kitchenRunning ? t('停止游戏') : t('启动游戏')"
          @click.stop="toggleGame">
          <n-spin v-if="launching" :size="14" />
          <StopOutline v-else-if="kitchenRunning" width="15" height="15" />
          <PlayOutline v-else width="15" height="15" />
        </button>
        <span v-if="gameHover" class="brand-game-hint">
          {{ kitchenRunning ? t('停止游戏') : t('启动游戏') }}
        </span>
      </div>
      <div class="brand-text" data-tauri-drag-region>
        <div class="brand-title-row">
          <strong>{{ t('OC2 模组管理器') }}</strong>
          <span v-if="props.version" class="brand-version">v{{ props.version }}</span>
        </div>
        <small>{{ t('游戏模组管理器') }}</small>
      </div>
    </div>

    <div class="spacer" data-tauri-drag-region></div>

    <!-- 界面缩放：下拉选择（与语言下拉一致） -->
    <n-dropdown :options="zoomOptions" :value="zoomKey" trigger="click" @select="onZoomSelect">
      <button class="pill" type="button" :title="t('界面缩放')">
        <ContrastOutline width="15" height="15" />
        <span>{{ zoomPercent }}%</span>
        <span class="caret">▾</span>
      </button>
    </n-dropdown>

    <!-- 语言 -->
    <n-dropdown :options="localeOptions.map(o => ({ label: o.label, key: o.value }))" :value="locale" trigger="click"
      @select="(key: Locale) => setLocale(key)">
      <button class="pill" type="button" :title="t('界面语言')">
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
    <n-popover trigger="click" placement="bottom-end" :show-arrow="false" raw>
      <template #trigger>
        <button class="pill icon-only" type="button" :title="t('主题色')" @contextmenu.prevent="cycleAccent">
          <span class="swatch" :style="{ background: accentSwatch }"></span>
        </button>
      </template>
      <div class="accent-grid" role="listbox" :aria-label="t('主题色')">
        <button v-for="entry in ACCENTS" :key="entry.key" type="button" class="accent-cell"
          :class="{ active: entry.key === accentKey }" role="option" :aria-selected="entry.key === accentKey"
          :title="accentLabel(entry.key)" :style="{ background: entry.swatch }" @click="setAccent(entry.key)" />
      </div>
    </n-popover>

    <!-- 窗口控制：仅真实 Tauri 窗口显示 -->
    <template v-if="canControlWindow">
      <span class="sep"></span>

      <button class="win-btn" type="button" :title="t('最小化')" :aria-label="t('最小化')" @click="minimize">
        <RemoveOutline width="16" height="16" />
      </button>
      <button class="win-btn" type="button" :title="maximized ? t('还原') : t('最大化')"
        :aria-label="maximized ? t('还原') : t('最大化')" @click="toggleMaximize">
        <ContractOutline v-if="maximized" width="15" height="15" />
        <SquareOutline v-else width="14" height="14" />
      </button>
      <button class="win-btn danger" type="button" :title="t('关闭')" :aria-label="t('关闭')" @click="close">
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
