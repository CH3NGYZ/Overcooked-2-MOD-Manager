<script setup lang="ts">
import { computed, ref } from 'vue'
import { NDropdown, NPopover } from 'naive-ui'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  CloseOutline, ContrastOutline, ContractOutline, LanguageOutline, MoonOutline, RemoveOutline,
  SquareOutline, SunnyOutline,
} from '@vicons/ionicons5'
import { ACCENTS, accentKey, cycleAccent, isDark, setAccent, setThemeMode, themeMode } from '../theme'
import { currentLocaleLabel, locale, localeOptions, setLocale, t, type Locale } from '../i18n'
import { useUiZoom } from '../composables/useUiZoom'
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
    <!-- 品牌 -->
    <div class="brand" data-tauri-drag-region>
      <img class="brand-mark" :src="appIcon" alt="" />
      <div class="brand-text" data-tauri-drag-region>
        <strong>{{ t('OC2 模组管理器') }}</strong>
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
  height: 56px;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 8px 0 14px;
  background: var(--oc2-rail);
  border-bottom: 1px solid var(--oc2-rail-line);
  user-select: none;
}
.brand {
  display: flex;
  align-items: center;
  gap: 11px;
  min-width: 0;
}
.brand-mark {
  width: 34px;
  height: 34px;
  flex: 0 0 34px;
  border-radius: 9px;
  object-fit: cover;
  display: block;
}
.brand-text { display: flex; flex-direction: column; min-width: 0; }
.brand-text strong { font-size: 14.5px; font-weight: 700; color: var(--oc2-text-1); }
.brand-text small { font-size: 11px; color: var(--oc2-text-3); margin-top: 1px; }
.spacer { flex: 1; height: 100%; }
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
}
.pill:hover { border-color: var(--oc2-accent); color: var(--oc2-text-1); }
.pill.icon-only { padding: 0; width: 32px; justify-content: center; }
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
