<script setup lang="ts">
/**
 * MOD 改名（多彩名字）：把逐字符颜色/字号编译成 MOD 可识别的富文本标签串，
 * 可复制或直接推送给正在运行的多彩名字 MOD。
 * 生成算法在 src/colorfulName.ts，推送走 Rust 命令（前端 CSP 不允许直连本地端口）。
 */
import { computed, nextTick, onBeforeUnmount, ref } from 'vue'
import { useMessage } from 'naive-ui'
import {
  NButton, NInput, NInputNumber, NSlider, NSpin, NTag,
} from 'naive-ui'
import { ColorWandOutline, CopyOutline, CloudUploadOutline, RefreshOutline } from '@vicons/ionicons5'
import {
  DEFAULT_COLOR, DEFAULT_SIZE, MAX_NAME_BYTES, buildShortestRichText, isNameOverLimit, utf16LeBytes,
} from '../colorfulName'
import { api } from '../services/tauri'
import { t } from '../i18n'

const message = useMessage()

/** MOD 内置的常用调色板（与游戏内命名习惯一致）。 */
const PALETTE = [
  '#000000', '#ffffff', '#ffa43b', '#5fa6de', '#ff0000',
  '#ffcc00', '#33cc33', '#0066ff', '#9933ff', '#ff66cc',
  '#666666', '#cccccc', '#990000', '#996600', '#999900',
  '#006600', '#006666', '#000099', '#660066', '#cc0066',
]

const rawName = ref('')
const started = ref(false)
const name = ref('')
const colors = ref<string[]>([])
const sizes = ref<number[]>([])
const selected = ref<{ start: number; end: number } | null>(null)
const picker = ref(DEFAULT_COLOR)
const sizeStep = ref(0)
const pushing = ref(false)
const previewEl = ref<HTMLElement | null>(null)

/** 逐字符样式：颜色、字号数组与字符一一对应。 */
const chars = computed(() => [...name.value])
const code = computed(() =>
  buildShortestRichText(chars.value.map((char, index) => ({
    char,
    color: colors.value[index] ?? DEFAULT_COLOR,
    size: sizes.value[index] ?? DEFAULT_SIZE,
  }))),
)
const byteLength = computed(() => utf16LeBytes(code.value))
const overLimit = computed(() => isNameOverLimit(code.value))
const percent = computed(() => Math.min(100, Math.round((byteLength.value / MAX_NAME_BYTES) * 100)))

function startEdit() {
  const text = rawName.value.trim()
  if (!text) { message.warning(t('请输入名字')); return }
  name.value = text
  colors.value = chars.value.map(() => DEFAULT_COLOR)
  sizes.value = chars.value.map(() => DEFAULT_SIZE)
  selected.value = null
  sizeStep.value = 0
  picker.value = DEFAULT_COLOR
  started.value = true
  void nextTick(clearNativeSelection)
}

function resetAll() {
  started.value = false
  name.value = ''
  colors.value = []
  sizes.value = []
  selected.value = null
}

/** 读取预览区里用户实际选中的字符范围。 */
function readSelection(): { start: number; end: number } | null {
  const element = previewEl.value
  const selection = window.getSelection()
  if (!element || !selection || !selection.rangeCount || selection.isCollapsed) return null
  const range = selection.getRangeAt(0)
  if (!element.contains(range.startContainer) || !element.contains(range.endContainer)) return null
  const spans = [...element.querySelectorAll<HTMLElement>('[data-index]')]
  const indexOf = (node: Node) => {
    const host = node.nodeType === Node.TEXT_NODE ? node.parentElement : (node as HTMLElement)
    const span = host?.closest<HTMLElement>('[data-index]')
    return span ? spans.indexOf(span) : -1
  }
  const start = indexOf(range.startContainer)
  const end = indexOf(range.endContainer)
  if (start < 0 || end < 0) return null
  return { start: Math.min(start, end), end: Math.max(start, end) + 1 }
}

function updateSelection() {
  const range = readSelection()
  if (range) {
    selected.value = range
    // 同步第一个字符的样式到控件，方便接着微调
    picker.value = colors.value[range.start] ?? DEFAULT_COLOR
    sizeStep.value = 0
  }
}

function clearNativeSelection() {
  window.getSelection()?.removeAllRanges()
}

/** 只改颜色，不改字号。 */
function applyColor(color: string) {
  const range = selected.value
  if (!range) { message.warning(t('请先选择要修改的文字')); return }
  picker.value = color
  const next = [...colors.value]
  for (let index = range.start; index < range.end; index += 1) next[index] = color.toLowerCase()
  colors.value = next
}

/** 字号按「基准 + 增量」应用，避免连续拖动叠加。 */
function applySize(step: number) {
  const range = selected.value
  if (!range) { message.warning(t('请先选择要修改的文字')); return }
  const next = [...sizes.value]
  for (let index = range.start; index < range.end; index += 1) {
    const base = sizes.value[index] ?? DEFAULT_SIZE
    next[index] = Math.max(1, Math.min(71, base + step))
  }
  sizes.value = next
  sizeStep.value = 0
}

function resetSelectionStyle() {
  const range = selected.value
  if (!range) { message.warning(t('请先选择要修改的文字')); return }
  const nextColors = [...colors.value]
  const nextSizes = [...sizes.value]
  for (let index = range.start; index < range.end; index += 1) {
    nextColors[index] = DEFAULT_COLOR
    nextSizes[index] = DEFAULT_SIZE
  }
  colors.value = nextColors
  sizes.value = nextSizes
  picker.value = DEFAULT_COLOR
  sizeStep.value = 0
}

async function copyCode() {
  if (overLimit.value) { message.error(t('名字代码超过限制')); return }
  try {
    await api.copyText(code.value)
    message.success(t('已复制，可粘贴到 MOD 菜单'))
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  }
}

async function pushToMod() {
  if (overLimit.value) { message.error(t('名字代码超过限制')); return }
  pushing.value = true
  try {
    const result = await api.pushColorfulName(code.value)
    result.ok ? message.success(result.message || t('已推送到 MOD')) : message.error(result.message)
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    pushing.value = false
  }
}

onBeforeUnmount(clearNativeSelection)
</script>

<template>
  <div class="colorful">
    <!-- 第一步：输入原始名字 -->
    <section class="panel">
      <div class="panel-head">
        <b>{{ t('输入名字') }}</b>
        <span class="muted">{{ t('支持任意字符，例如中文、英文与符号') }}</span>
      </div>
      <n-input v-model:value="rawName" type="textarea" :autosize="{ minRows: 2, maxRows: 4 }"
        :placeholder="t('在这里输入要修改的名字')" :disabled="started" @keydown.enter.prevent="startEdit" />
      <div class="row-actions">
        <n-button v-if="!started" type="primary" @click="startEdit">
          <ColorWandOutline width="15" height="15" />{{ t('开始自定义') }}
        </n-button>
        <template v-else>
          <n-button secondary @click="resetAll">
            <RefreshOutline width="15" height="15" />{{ t('重新输入') }}
          </n-button>
          <n-button type="primary" :loading="pushing" :disabled="overLimit" @click="pushToMod">
            <CloudUploadOutline width="15" height="15" />{{ t('推送到 MOD') }}
          </n-button>
          <n-button secondary :disabled="overLimit" @click="copyCode">
            <CopyOutline width="15" height="15" />{{ t('复制代码') }}
          </n-button>
        </template>
      </div>
    </section>

    <template v-if="started">
      <!-- 第二步：选中文字并调整样式 -->
      <section class="panel">
        <div class="panel-head">
          <b>{{ t('选择要修改的文字') }}</b>
          <span class="muted">{{ t('拖动选中预览里的字符，再应用颜色或字号') }}</span>
        </div>
        <div ref="previewEl" class="preview" @mouseup="updateSelection" @keyup="updateSelection">
          <span v-for="(char, index) in chars" :key="index" :data-index="index" class="preview-char"
            :style="{ color: colors[index], fontSize: `${sizes[index]}px` }">{{ char }}</span>
        </div>

        <div class="controls">
          <div class="control-block">
            <span class="control-label">{{ t('选择颜色') }}</span>
            <div class="palette">
              <button v-for="color in PALETTE" :key="color" class="swatch"
                :class="{ active: picker.toLowerCase() === color }" :style="{ background: color }"
                :title="color" @mousedown.prevent="applyColor(color)" />
              <!-- 自定义颜色：原生取色器做成和色板同尺寸的方块 -->
              <label class="swatch swatch-custom" :title="t('更多颜色')"
                :style="{ background: PALETTE.includes(picker.toLowerCase()) ? 'conic-gradient(#f00, #ff0, #0f0, #0ff, #00f, #f0f, #f00)' : picker }">
                <input type="color" :value="picker" class="swatch-input"
                  @input="(event: Event) => applyColor((event.target as HTMLInputElement).value)" />
              </label>
            </div>
          </div>

          <div class="control-block">
            <span class="control-label">{{ t('字号增量') }}</span>
            <div class="size-row">
              <n-slider :value="sizeStep" :min="-34" :max="36" :step="1" class="size-slider"
                @update:value="(value: number) => (sizeStep = value)"
                @dragend="applySize(sizeStep)" />
              <n-input-number :value="sizeStep" size="small" :min="-34" :max="36" class="size-input"
                @update:value="(value: number | null) => applySize(value ?? 0)" />
              <n-button size="small" secondary @click="resetSelectionStyle">
                <RefreshOutline width="14" height="14" />{{ t('重置样式') }}
              </n-button>
            </div>
          </div>
        </div>
      </section>

      <!-- 第三步：生成结果 -->
      <section class="panel">
        <div class="panel-head">
          <b>{{ t('生成的名字代码') }}</b>
          <n-tag size="tiny" :bordered="false" class="state-tag"
            :class="overLimit ? 'state-tag--danger' : 'state-tag--muted'">
            {{ byteLength }} / {{ MAX_NAME_BYTES }} B
          </n-tag>
        </div>
        <n-input :value="code" type="textarea" readonly :autosize="{ minRows: 2, maxRows: 6 }" class="code" />
        <div class="meter">
          <div class="meter-fill" :class="{ over: overLimit }" :style="{ width: `${percent}%` }" />
        </div>
        <p v-if="overLimit" class="over muted">{{ t('名字代码超过 MOD 的 238 字节限制，请减少代码长度') }}</p>
        <p v-else class="muted">{{ t('复制后粘贴到游戏内 MOD 菜单，或直接推送到正在运行的 MOD') }}</p>
      </section>
    </template>

    <n-spin v-if="pushing" class="push-hint" size="small" />
  </div>
</template>

<style scoped>
.colorful {
  display: flex;
  flex-direction: column;
  gap: 14px;
  width: 100%;
}
.panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 14px 16px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
}
.panel-head {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}
.panel-head b { font-size: 13.5px; color: var(--oc2-text-1); }
.panel-head .muted { font-size: 11.5px; }
.row-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
}
/* 预览区：模拟游戏内名字输入框的浅色底，黑色文字才看得见；
   逐字符 span 按行内文字流排布，拖动选择与普通文本一致。 */
.preview {
  display: block;
  min-height: 74px;
  padding: 12px 14px;
  border: 1px dashed var(--oc2-row-line);
  border-radius: 10px;
  background: #f2f4f7;
  white-space: pre-wrap;
  line-height: 1.5;
  word-break: break-word;
  user-select: text;
  cursor: text;
}
.preview-char { white-space: pre; }
.controls {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.control-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.control-label { font-size: 12px; color: var(--oc2-text-3); }
.palette {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}
.swatch {
  position: relative;
  width: 24px;
  height: 24px;
  padding: 0;
  border: 2px solid var(--oc2-row-line);
  border-radius: 6px;
  cursor: pointer;
  transition: transform .14s ease, border-color .14s ease;
}
.swatch:hover { transform: scale(1.12); border-color: var(--oc2-accent); }
.swatch.active {
  border-color: var(--oc2-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 40%, transparent);
}
/* 自定义取色：把 input 铺满整个方块并隐藏原生外观 */
.swatch-custom { display: block; overflow: hidden; }
.swatch-input {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  padding: 0;
  border: 0;
  opacity: 0;
  cursor: pointer;
}
.size-row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.size-slider { flex: 1; min-width: 120px; }
.size-input { width: 90px; flex: 0 0 auto; }
.code :deep(textarea) { font-family: Consolas, monospace; font-size: 12px; }
.meter {
  height: 4px;
  border-radius: 999px;
  background: var(--oc2-sunken);
  overflow: hidden;
}
.meter-fill {
  height: 100%;
  border-radius: inherit;
  background: var(--oc2-accent);
  transition: width .18s ease, background-color .18s ease;
}
.meter-fill.over { background: #ef4444; }
.over { color: #f87171; }
.push-hint { align-self: center; }
</style>
