<script setup lang="ts">
import { computed, ref } from 'vue'
import { NDropdown } from 'naive-ui'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  CloseOutline, ContractOutline, LanguageOutline, MoonOutline, RemoveOutline,
  SquareOutline, SunnyOutline,
} from '@vicons/ionicons5'
import { ACCENTS, accentKey, cycleAccent, isDark, setAccent, setThemeMode, themeMode } from '../theme'
import { currentLocaleLabel, locale, localeOptions, setLocale, t, type Locale } from '../i18n'
import { useUiZoom } from '../composables/useUiZoom'
import appIcon from '../assets/app-icon.png'

const props = defineProps<{ version: string }>()

const appWindow = getCurrentWindow()
const maximized = ref(false)
const { zoomPercent, setZoom } = useUiZoom()

void appWindow.isMaximized().then((value) => { maximized.value = value }).catch(() => undefined)
let unlistenResize: (() => void) | null = null
void appWindow.onResized(async () => {
  maximized.value = await appWindow.isMaximized().catch(() => false)
}).then((un) => { unlistenResize = un })

function onUnmountedCleanup() { unlistenResize?.() }

const accentOptions = computed(() => ACCENTS.map((entry) => ({
  key: entry.key,
  label: t(entry.key === 'neutral' ? t('中性') : entry.key === 'blue' ? t('蓝色') : entry.key === 'green' ? t('绿色') : entry.key === 'purple' ? t('紫色') : t('红色')),
  color: entry.swatch,
})))

const themeLabel = computed(() => (isDark.value ? t('深色') : t('浅色')))

// 窗口控制
async function minimize() { await appWindow.minimize() }
async function toggleMaximize() {
  await appWindow.toggleMaximize()
  maximized.value = await appWindow.isMaximized().catch(() => false)
}
async function close() { await appWindow.close() }

// 缩放：25% 步进的横拖动条，100%–200%，默认 150%
const ZOOM_STEPS = [100, 125, 150, 175, 200] as const
const zoomIndex = computed(() => {
  const index = ZOOM_STEPS.findIndex((step) => step === zoomPercent.value)
  return index === -1 ? 2 : index
})
function onZoomStep(index: number) {
  void setZoom(ZOOM_STEPS[Math.min(ZOOM_STEPS.length - 1, Math.max(0, index))] / 100)
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

    <!-- 界面缩放：25% 步进的节点滑条（100%–200%，默认 150%） -->
    <div class="zoom" :title="t('界面缩放')">
      <div class="zoom-track">
        <div class="zoom-fill" :style="{ width: `${(zoomIndex / (ZOOM_STEPS.length - 1)) * 100}%` }"></div>
        <button v-for="(step, index) in ZOOM_STEPS" :key="step" type="button" class="zoom-node"
          :class="{ on: index <= zoomIndex, current: index === zoomIndex }" :aria-label="`${step}%`"
          :aria-pressed="index === zoomIndex" @click="onZoomStep(index)">
          <span>{{ step }}</span>
        </button>
      </div>
      <span class="zoom-value">{{ zoomPercent }}%</span>
    </div>

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

    <!-- 主题色 -->
    <n-dropdown :options="accentOptions" trigger="click" @select="(key: any) => setAccent(key)">
      <button class="pill icon-only" type="button" :title="t('主题色')" @contextmenu.prevent="cycleAccent">
        <span class="swatch" :style="{ background: ACCENTS.find(a => a.key === accentKey)?.swatch }"></span>
      </button>
    </n-dropdown>

    <span class="sep"></span>

    <!-- 窗口控制 -->
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

/* 缩放滑条：灰色轨道 + 已选段高亮 + 节点圆点（当前节点加环） */
.zoom {
  display: flex;
  align-items: center;
  gap: 9px;
  padding: 0 4px;
  height: 32px;
}
.zoom-track {
  position: relative;
  display: flex;
  align-items: center;
  width: 168px;
  height: 100%;
}
/* 轨道底线 / 已选段：靠节点中心对齐 */
.zoom-track::before,
.zoom-fill {
  content: '';
  position: absolute;
  top: 50%;
  height: 3px;
  margin-top: -1.5px;
  border-radius: 2px;
}
.zoom-track::before {
  left: 0;
  right: 0;
  background: var(--oc2-rail-line);
}
.zoom-fill {
  left: 0;
  background: var(--oc2-accent);
  transition: width .14s ease;
}
.zoom-node {
  position: relative;
  z-index: 1;
  flex: 1 1 0;
  display: grid;
  place-items: center;
  padding: 0;
  border: 0;
  background: transparent;
  cursor: pointer;
}
.zoom-node > span {
  width: 15px;
  height: 15px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  background: var(--oc2-sunken);
  outline: 1.5px solid var(--oc2-rail-line);
  outline-offset: 0;
  font-size: 0;              /* 节点只显示圆点，数值在右侧统一显示 */
  transition: background-color .14s ease, outline-color .14s ease, transform .14s ease;
}
.zoom-node.on > span {
  background: var(--oc2-accent);
  outline-color: var(--oc2-accent);
}
.zoom-node.current > span {
  outline-width: 2px;
  outline-style: solid;
  outline-color: var(--oc2-accent);
  box-shadow: 0 0 0 3px color-mix(in srgb, var(--oc2-accent) 24%, transparent);
  transform: scale(1.12);
}
.zoom-node:hover > span {
  outline-color: var(--oc2-accent);
}
.zoom-node:focus-visible {
  outline: none;
}
.zoom-node:focus-visible > span {
  outline-width: 2px;
  outline-color: var(--oc2-text-1);
}
.zoom-value {
  min-width: 38px;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  color: var(--oc2-text-2);
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
