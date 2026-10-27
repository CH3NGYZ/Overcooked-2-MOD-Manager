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

/**
 * Naive UI 变量覆盖：随 mode + accent 变化。
 *
 * 这是**唯一**的 Naive 主题入口：所有 Naive 组件的配色都从这里下发，
 * 组件里不再各自覆盖颜色。项目自定义的语义色（状态标签、安装按钮）
 * 无法用 Naive 的标准 type 表达，它们通过 `--oc2-*` CSS 变量共享
 * 同一个 accent 来源，见 applyThemeCssVars。
 */
export const themeOverrides = computed<GlobalThemeOverrides>(() => {
  const surface = surfaces.value
  const main = accent.value
  const contrast = accentContrast(main.color, themeMode.value)
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
      infoColor: main.color,
      infoColorHover: main.hover,
      infoColorPressed: main.pressed,
      successColor: main.color,
      successColorHover: main.hover,
      successColorPressed: main.pressed,
      warningColor: contrast,
      warningColorHover: hoverOf(contrast),
      warningColorPressed: hoverOf(contrast),
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
    /*
     * Tag：`type="info"` 跟随主题色、`type="warning"` 跟随对比色。
     * 这样页面里用 Naive 原生 Tag 类型的地方不必再写 CSS 覆盖，
     * 只保留项目自定义的 .state-tag--* 语义类（重复/低版本等无法映射到 type 的状态）。
     */
    Tag: {
      colorInfo: tint(main.color, surface.card, 0.82),
      colorInfoBorder: main.color,
      textColorInfo: main.color,
      colorWarning: tint(contrast, surface.card, 0.84),
      colorWarningBorder: contrast,
      textColorWarning: contrast,
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
 * 把颜色按比例混入底色，得到同色系的低饱和背景。
 * 前景色保持纯色，背景用混合色 —— 这是状态标签的通用构造方式。
 */
function tint(color: string, base: string, weight: number): string {
  const front = hexToHsl(color)
  const back = hexToHsl(base)
  if (!front || !back) return color
  const [h1, s1, l1] = front
  const [, , l2] = back
  return hslToHex(h1, s1, l1 * (1 - weight) + l2 * weight)
}

/** `n-tag` 的 `color` 属性形状（Naive 原生类型，避免自绘标签配色）。 */
export interface TagColorValue {
  color: string
  borderColor: string
  textColor: string
}

/**
 * 语义色调：全站唯一的「严重程度」刻度。
 *
 * 标签和按钮共用这一个刻度，颜色强度直接对应用户需要多在意：
 *
 *   accent       正常   —— 主题色。最新版、已安装、部分启用（一切正常）
 *   mild         轻微   —— 对比色最弱档。低版本副本（提示性，通常顺手卸载）
 *   moderate     中度   —— 对比色中档。可更新、预发布（需要动作但不紧急）
 *   severe       重度   —— 对比色最强档。重复（同 GUID 冲突，必须处理）
 *   critical     严重   —— 独立的红色。来源未知、仓库缺失、加载警告
 *   muted        中性   —— 灰色。未安装、未绑定、已禁用、版本未知
 *
 * 每档只在这里定义一次「强调色 + 背景混合权重」，
 * 标签的浅底描边样式与按钮的实心样式都由它派生（见 toneStyles）。
 */
export type TagTone = 'accent' | 'mild' | 'moderate' | 'severe' | 'critical' | 'muted'

/**
 * 每档的派生强度。
 *
 * `tagTint` 越大 → 标签底色越接近卡片色（越淡）；
 * `buttonTint` 更大 → 按钮用同色系的浅底描边（而非实心），用于「提示 + 可点击」
 * 这类不该比标签更抢眼的动作（如刷新失败的「刷新」按钮）。
 */
interface ToneSpec {
  /** 强调色：文字、边框、实心按钮底色都用它。 */
  color: string
  /** 标签背景混入卡片色的权重（越大越淡）。 */
  tagTint: number
  /** 边框混入卡片色的权重；不设则用纯强调色描边。 */
  tagBorderTint?: number
}

/** 深色/浅色无关的固定红：表达「数据/来源不可信」，不该随主题变浅。 */
function criticalColor(mode: ThemeMode): string {
  return mode === 'dark' ? '#f87171' : '#dc2626'
}

/** 计算每档的语义规格。标签与按钮的配色都从这一份派生。 */
function toneSpecs(): Record<TagTone, ToneSpec> {
  const surface = surfaces.value
  const main = accent.value
  const contrast = accentContrast(main.color, themeMode.value)
  const muted = surface.text3
  return {
    accent: { color: main.color, tagTint: 0.82 },
    mild: { color: tint(contrast, muted, 0.32), tagTint: 0.9, tagBorderTint: 0.6 },
    moderate: { color: contrast, tagTint: 0.84 },
    severe: { color: contrast, tagTint: 0.74 },
    critical: { color: criticalColor(themeMode.value), tagTint: 0.8 },
    muted: { color: muted, tagTint: 0.86, tagBorderTint: 0.7 },
  }
}

/**
 * 状态标签的统一配色表。
 *
 * 所有页面的状态标签都从这里取值，通过 Naive `n-tag` 的原生 `:color`
 * 属性下发 —— 不需要任何自定义 CSS 类，换主题色时自动跟随。
 */
export const statusToneColors = computed<Record<TagTone, TagColorValue>>(() => {
  const surface = surfaces.value
  const specs = toneSpecs()
  const build = (tone: TagTone): TagColorValue => {
    const spec = specs[tone]
    return {
      color: tint(spec.color, surface.card, spec.tagTint),
      borderColor: spec.tagBorderTint === undefined
        ? spec.color
        : tint(spec.color, surface.card, spec.tagBorderTint),
      textColor: spec.color,
    }
  }
  return {
    accent: build('accent'),
    mild: build('mild'),
    moderate: build('moderate'),
    severe: build('severe'),
    critical: build('critical'),
    muted: build('muted'),
  }
})

/**
 * 按钮的统一配色：与标签同一个 tone 刻度，但分成两种视觉强度。
 *
 *   solid  —— 实心。用于「这就是主操作」（安装 / 重装 / 立即安装）。
 *   soft   —— 浅底 + 描边 + 同色文字。用于「提示可以点这里重试」
 *             （刷新失败的刷新按钮）——不该比旁边的标签更抢眼。
 *
 * 返回值直接绑到 `n-button` 的 `:style`，Naive 的按钮配色变量写在
 * 内联 style 上，所以内联写入才能稳定覆盖，不用 CSS 类加 !important。
 */
export interface ButtonColorValue {
  '--n-color': string
  '--n-color-hover': string
  '--n-color-pressed': string
  '--n-color-focus': string
  '--n-text-color': string
  '--n-text-color-hover': string
  '--n-text-color-pressed': string
  '--n-text-color-focus': string
  '--n-border': string
  '--n-border-hover': string
  '--n-border-pressed': string
  '--n-border-focus': string
}

export type ButtonVariant = 'solid' | 'soft'

/**
 * 取某个 tone 的按钮配色。
 *
 * @param variant solid = 实心；soft = 浅底描边（同级标签的低调版）
 */
export function buttonColorFor(tone: TagTone, variant: ButtonVariant = 'solid'): ButtonColorValue {
  const surface = surfaces.value
  const spec = toneSpecs()[tone]
  const strong = spec.color
  const strongHover = hoverOf(spec.color)

  if (variant === 'solid') {
    // 实心按钮：底色是强调色本身，文字用「在强调色上可读」的颜色。
    const ink = readableOn(strong)
    return {
      '--n-color': strong,
      '--n-color-hover': strongHover,
      '--n-color-pressed': strongHover,
      '--n-color-focus': strongHover,
      '--n-text-color': ink,
      '--n-text-color-hover': ink,
      '--n-text-color-pressed': ink,
      '--n-text-color-focus': ink,
      '--n-border': `1px solid ${strong}`,
      '--n-border-hover': `1px solid ${strongHover}`,
      '--n-border-pressed': `1px solid ${strongHover}`,
      '--n-border-focus': `1px solid ${strongHover}`,
    }
  }

  // 浅底描边：直接复用该档标签的背景色，保证「同档 = 同色」
  const base = tint(strong, surface.card, spec.tagTint)
  return {
    '--n-color': base,
    '--n-color-hover': tint(strong, surface.card, Math.max(0, spec.tagTint - 0.1)),
    '--n-color-pressed': tint(strong, surface.card, Math.max(0, spec.tagTint - 0.16)),
    '--n-color-focus': tint(strong, surface.card, Math.max(0, spec.tagTint - 0.1)),
    '--n-text-color': strong,
    '--n-text-color-hover': strongHover,
    '--n-text-color-pressed': strongHover,
    '--n-text-color-focus': strongHover,
    '--n-border': `1px solid ${strong}`,
    '--n-border-hover': `1px solid ${strongHover}`,
    '--n-border-pressed': `1px solid ${strongHover}`,
    '--n-border-focus': `1px solid ${strongHover}`,
  }
}

/**
 * 在给定背景上可读的文字色（深色或白色）。
 * 用 HSL 亮度判断即可 —— 强调色都是高饱和纯色，
 * 亮度超过一半时配深色文字，否则配白字。
 */
function readableOn(background: string): string {
  const hsl = hexToHsl(background)
  if (!hsl) return '#ffffff'
  const [, , lightness] = hsl
  return lightness > 0.55 ? '#10151a' : '#ffffff'
}

/** 旧色调名 → 现行五档。只为读取旧快照兼容，新代码不要使用旧名。 */
const LEGACY_TONES: Record<string, TagTone> = {
  ok: 'accent',
  idle: 'accent',
  warn: 'moderate',
  danger: 'critical',
  violet: 'mild',
}

/** 取某个 tone 的标签配色（严格版：拼错的键在编译期报错）。 */
export function tagColorFor(tone: TagTone): TagColorValue {
  return statusToneColors.value[tone]
}

/**
 * 同 tagColorFor，但接受来自后端/旧快照的任意字符串。
 * 旧色调名映射到现行五档，完全无法识别的回退中性灰。
 */
export function tagColorForAny(tone: string): TagColorValue {
  const table = statusToneColors.value
  if (tone in table) return table[tone as TagTone]
  const mapped = LEGACY_TONES[tone]
  return mapped ? table[mapped] : table.muted
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
  /*
   * 每档语义色的 CSS 变量，供 scoped 样式使用（标签/按钮之外的自绘元素）。
   * 和 statusToneColors / buttonColorFor 同源，换主题时一起变。
   */
  const tones = statusToneColors.value
  for (const tone of Object.keys(tones) as TagTone[]) {
    const value = tones[tone]
    vars[`--oc2-tone-${tone}-bg`] = value.color
    vars[`--oc2-tone-${tone}-border`] = value.borderColor
    vars[`--oc2-tone-${tone}-text`] = value.textColor
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
