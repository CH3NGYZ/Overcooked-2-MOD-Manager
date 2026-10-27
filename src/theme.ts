import { computed, ref, watch } from 'vue'
import { darkTheme, type GlobalTheme, type GlobalThemeOverrides } from 'naive-ui'

/**
 * 主题能力（通用可扩展）：
 *   mode   —— 'dark' | 'light'（Naive UI 内置 darkTheme / null=light）
 *   accent —— 主色调色板键；新增颜色只需在 ACCENTS 里加一行。
 * 两项都持久化到 localStorage，改完立即作用到 NConfigProvider 与 CSS 变量。
 */

export type ThemeMode = 'dark' | 'light'
export type AccentKey =
  | 'slate' | 'gray' | 'rose'
  | 'red' | 'orange' | 'amber'
  | 'green' | 'teal' | 'blue'

const MODE_KEY = 'oc2-theme-mode'
const ACCENT_KEY = 'oc2-theme-accent'

interface Accent {
  key: AccentKey
  /** 调色板显示色 */
  swatch: string
  /** 主色三态（hover/pressed 由主色推导，保持统一） */
  color: string
  hover: string
  pressed: string
  /** 主色上的前景色（按钮文字） */
  onColor: string
}

/**
 * 经典九色（3×3 排布）：
 *   板岩 / 灰 / 玫红
 *   红 / 橙 / 琥珀
 *   绿 / 青 / 蓝
 * 颜色值取自 Tailwind 的 500 色阶，深浅适中、在明暗两种模式下都能看清。
 */
export const ACCENTS: Accent[] = [
  { key: 'slate', swatch: '#64748b', color: '#64748b', hover: '#7c8ba1', pressed: '#52606f', onColor: '#0c1116' },
  { key: 'gray', swatch: '#6b7280', color: '#6b7280', hover: '#858c99', pressed: '#565d68', onColor: '#0c1116' },
  { key: 'rose', swatch: '#f43f5e', color: '#f43f5e', hover: '#f76880', pressed: '#d92e4b', onColor: '#1a0609' },
  { key: 'red', swatch: '#ef4444', color: '#ef4444', hover: '#f36b6b', pressed: '#d13333', onColor: '#1a0808' },
  { key: 'orange', swatch: '#f97316', color: '#f97316', hover: '#fb8c3c', pressed: '#d95f0b', onColor: '#1c0d03' },
  { key: 'amber', swatch: '#f59e0b', color: '#f59e0b', hover: '#f7b13a', pressed: '#d48706', onColor: '#1a1103' },
  { key: 'green', swatch: '#22c55e', color: '#22c55e', hover: '#4ad07c', pressed: '#17a34a', onColor: '#04170c' },
  { key: 'teal', swatch: '#14b8a6', color: '#14b8a6', hover: '#3acdbd', pressed: '#0f9a8c', onColor: '#031614' },
  { key: 'blue', swatch: '#3b82f6', color: '#3b82f6', hover: '#609bf9', pressed: '#2563d9', onColor: '#040f1f' },
]

const ACCENT_KEYS = ACCENTS.map((entry) => entry.key as string)

export const themeMode = ref<ThemeMode>(localStorage.getItem(MODE_KEY) === 'light' ? 'light' : 'dark')
export const accentKey = ref<AccentKey>(
  ACCENT_KEYS.includes(localStorage.getItem(ACCENT_KEY) ?? '')
    ? (localStorage.getItem(ACCENT_KEY) as AccentKey)
    : 'teal',
)

watch(themeMode, (value) => localStorage.setItem(MODE_KEY, value))
watch(accentKey, (value) => localStorage.setItem(ACCENT_KEY, value))

export const accent = computed(() => ACCENTS.find((entry) => entry.key === accentKey.value) ?? ACCENTS[2])

/** Naive UI 主题对象：浅色用内置亮色（不传 theme）。 */
export const naiveTheme = computed<GlobalTheme | null>(() => (themeMode.value === 'dark' ? darkTheme : null))

export const isDark = computed(() => themeMode.value === 'dark')

/** 两种模式的表面色板，供组件内 scoped 样式与全局 CSS 变量使用。 */
const SURFACES = {
  dark: {
    body: '#131417', card: '#1e2025', popover: '#23252b', rail: '#16181c', railLine: '#232a33',
    row: '#1b1e24', rowLine: '#262c35', rowHover: '#21262e', sunken: '#181b20',
    text1: '#e8edf3', text2: '#c3cdd8', text3: '#8d9bab', text4: '#617081', divider: '#273341',
  },
  light: {
    body: '#f4f6fa', card: '#ffffff', popover: '#ffffff', rail: '#eef1f6', railLine: '#dbe1ea',
    row: '#ffffff', rowLine: '#e2e7ef', rowHover: '#f6f8fc', sunken: '#f0f3f8',
    text1: '#1c2430', text2: '#3f4b5b', text3: '#6b7789', text4: '#8f9aab', divider: '#e2e7ef',
  },
} as const

export const surfaces = computed(() => SURFACES[themeMode.value])

/** Naive UI 变量覆盖：随 mode + accent 变化。 */
export const themeOverrides = computed<GlobalThemeOverrides>(() => {
  const surface = surfaces.value
  const main = accent.value
  return {
    common: {
      fontFamily: "'Segoe UI', 'Microsoft YaHei', 'PingFang SC', sans-serif",
      borderRadius: '8px',
      bodyColor: surface.body,
      cardColor: surface.card,
      modalColor: surface.card,
      popoverColor: surface.popover,
      primaryColor: main.color,
      primaryColorHover: main.hover,
      primaryColorPressed: main.pressed,
      primaryColorSuppl: main.color,
      successColor: main.color,
      warningColor: '#e6b86a',
      errorColor: '#ee8585',
      textColorBase: surface.text1,
      textColor1: surface.text1,
      textColor2: surface.text2,
      textColor3: surface.text3,
      borderColor: surface.rowLine,
      dividerColor: surface.divider,
    },
    Card: { borderRadius: '10px' },
    /*
     * 分段切换（type="segment"）的轨道与胶囊在深色下默认都用 inputColor，
     * 两者同色 → 选中项几乎看不出来。这里改成亮色主题的层次思路：
     * 轨道用下沉底色、胶囊比卡片更亮的浮层色，选中文字用主题色。
     */
    Tabs: {
      colorSegment: surface.sunken,
      tabColorSegment: surface.popover,
      tabTextColorSegment: surface.text2,
      tabTextColorActiveSegment: main.color,
      tabTextColorHoverSegment: surface.text1,
    },
  }
})

/**
 * 把 `#rrggbb` 转成 HSL 三元组（h: 0-360, s/l: 0-1）。
 */
function hexToHsl(hex: string): [number, number, number] | null {
  const match = /^#?([0-9a-f]{6})$/i.exec(hex.trim())
  if (!match) return null
  const value = parseInt(match[1], 16)
  const r = ((value >> 16) & 0xff) / 255
  const g = ((value >> 8) & 0xff) / 255
  const b = (value & 0xff) / 255
  const max = Math.max(r, g, b)
  const min = Math.min(r, g, b)
  const l = (max + min) / 2
  if (max === min) return [0, 0, l]
  const d = max - min
  const s = l > 0.5 ? d / (2 - max - min) : d / (max + min)
  let h: number
  if (max === r) h = ((g - b) / d + (g < b ? 6 : 0)) / 6
  else if (max === g) h = ((b - r) / d + 2) / 6
  else h = ((r - g) / d + 4) / 6
  return [Math.round(h * 360), s, l]
}

function hslToHex(h: number, s: number, l: number): string {
  const hue = ((h % 360) + 360) % 360 / 360
  const clamp = (value: number) => Math.min(1, Math.max(0, value))
  const s2 = clamp(s)
  const l2 = clamp(l)
  const q = l2 < 0.5 ? l2 * (1 + s2) : l2 + s2 - l2 * s2
  const p = 2 * l2 - q
  const channel = (t: number) => {
    let value = t
    if (value < 0) value += 1
    if (value > 1) value -= 1
    if (value < 1 / 6) return p + (q - p) * 6 * value
    if (value < 1 / 2) return q
    if (value < 2 / 3) return p + (q - p) * (2 / 3 - value) * 6
    return p
  }
  const toByte = (value: number) =>
    Math.round(clamp(value) * 255).toString(16).padStart(2, '0')
  return `#${toByte(channel(hue + 1 / 3))}${toByte(channel(hue))}${toByte(channel(hue - 1 / 3))}`
}

/**
 * 主题色的对比色。
 *
 * 色相旋转 180° 得到互补色，同时把亮度拉回接近中段 —— 直接取互补色在
 * 深色主题下常偏暗（比如青色互补的红色），提亮后按钮文字才够清晰。
 * 亮度只在原色过亮/过暗时收敛，正常色阶保持原有观感。
 */
export function accentContrast(color: string, mode: ThemeMode): string {
  const hsl = hexToHsl(color)
  if (!hsl) return color
  const [h, s, l] = hsl
  // 深色主题偏亮一档、浅色主题偏深一档，保证与背景有足够对比
  const target = mode === 'dark'
    ? Math.min(0.62, Math.max(0.52, l))
    : Math.min(0.46, Math.max(0.36, l))
  return hslToHex(h + 180, s, target)
}

/** 由基色推导一个 hover 态（提亮约 12%）。 */
function hoverOf(color: string): string {
  const hsl = hexToHsl(color)
  if (!hsl) return color
  const [h, s, l] = hsl
  return hslToHex(h, s, Math.min(0.78, l + 0.12))
}

/**
 * 把主题写成 CSS 变量，供 scoped 样式使用（`var(--oc2-*)`）。
 * 这样新增主题色/模式不需要改任何组件样式。
 */
export function applyThemeCssVars() {
  const surface = surfaces.value
  const main = accent.value
  const root = document.documentElement
  const vars: Record<string, string> = {
    '--oc2-body': surface.body,
    '--oc2-card': surface.card,
    '--oc2-popover': surface.popover,
    '--oc2-rail': surface.rail,
    '--oc2-rail-line': surface.railLine,
    '--oc2-row': surface.row,
    '--oc2-row-line': surface.rowLine,
    '--oc2-row-hover': surface.rowHover,
    '--oc2-sunken': surface.sunken,
    '--oc2-text-1': surface.text1,
    '--oc2-text-2': surface.text2,
    '--oc2-text-3': surface.text3,
    '--oc2-text-4': surface.text4,
    '--oc2-divider': surface.divider,
    '--oc2-accent': main.color,
    '--oc2-accent-hover': main.hover,
    '--oc2-accent-on': main.onColor,
    /*
     * 主题色的「对比色」：用于需要与主色区分、但同样醒目的一套语义
     * （安装页的「重装 / 更新」按钮与「可更新」标签）。
     * 取主色旋转 180° 的色相并保持相同的饱和度/亮度档位，
     * 因此切换主题色时它跟着变，而不是写死成某个颜色。
     */
    '--oc2-accent-contrast': accentContrast(main.color, themeMode.value),
    '--oc2-accent-contrast-hover': hoverOf(accentContrast(main.color, themeMode.value)),
  }
  for (const [name, value] of Object.entries(vars)) root.style.setProperty(name, value)
  root.style.colorScheme = themeMode.value
  document.documentElement.classList.toggle('dark', themeMode.value === 'dark')
}

watch([themeMode, accentKey], applyThemeCssVars, { immediate: true })

export function setThemeMode(mode: ThemeMode) {
  themeMode.value = mode
}

export function setAccent(key: AccentKey) {
  accentKey.value = key
}

export function cycleAccent() {
  const index = ACCENTS.findIndex((entry) => entry.key === accentKey.value)
  accentKey.value = ACCENTS[(index + 1) % ACCENTS.length].key
}

export function toggleMode() {
  themeMode.value = themeMode.value === 'dark' ? 'light' : 'dark'
}
