<script setup lang="ts">
import { computed, ref } from 'vue'
import { NDropdown, NPopselect } from 'naive-ui'
import { getCurrentWindow } from '@tauri-apps/api/window'
import {
  CloseOutline, ContractOutline, LanguageOutline, MoonOutline, RemoveOutline,
  SquareOutline, SunnyOutline, ContrastOutline,
} from '@vicons/ionicons5'
import { ACCENTS, accentKey, cycleAccent, isDark, setAccent, setThemeMode, themeMode } from '../theme'
import { currentLocaleLabel, locale, localeOptions, setLocale, t, type Locale } from '../i18n'
import { useUiZoom } from '../composables/useUiZoom'

const props = defineProps<{ version: string }>()

const appWindow = getCurrentWindow()
const maximized = ref(false)
const { zoomPercent, zoomIn, zoomOut, resetZoom, canZoomIn, canZoomOut } = useUiZoom()

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

// 缩放
const zoomMenu = computed(() => [
  { label: '100%', key: 100 }, { label: '125%', key: 125 }, { label: '150%', key: 150 },
  { label: '175%', key: 175 }, { label: '200%', key: 200 },
])
function chooseZoom(key: number) {
  const target = Math.round((key / 100) * 10) / 10
  while (zoomPercent.value < target * 100 && canZoomIn.value) zoomIn()
  while (zoomPercent.value > target * 100 && canZoomOut.value) zoomOut()
  if (zoomPercent.value !== target * 100) resetZoom()
}

defineExpose({ onUnmountedCleanup })
</script>

<template>
  <header class="titlebar" data-tauri-drag-region>
    <!-- 品牌 -->
    <div class="brand" data-tauri-drag-region>
      <div class="brand-mark">＋</div>
      <div class="brand-text" data-tauri-drag-region>
        <strong>{{ t('OC2 模组管理器') }}</strong>
        <small>{{ t('游戏模组管理器') }}</small>
      </div>
    </div>

    <div class="spacer" data-tauri-drag-region></div>

    <!-- 界面缩放 -->
    <n-popselect :options="zoomMenu" :value="zoomPercent" trigger="click" @update:value="chooseZoom">
      <button class="pill" type="button" :title="t('界面缩放')">
        <ContrastOutline width="15" height="15" />
        <span>{{ zoomPercent }}%</span>
        <span class="caret">▾</span>
      </button>
    </n-popselect>

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
  border-radius: 10px;
  display: grid;
  place-items: center;
  font-size: 19px;
  font-weight: 700;
  color: #fff;
  background: linear-gradient(135deg, var(--oc2-accent), var(--oc2-accent-hover));
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
