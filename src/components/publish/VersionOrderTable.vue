<script setup lang="ts">
import { computed, ref } from 'vue'
import { NButton, NInput, NTag } from 'naive-ui'
import { ReorderThreeOutline } from '@vicons/ionicons5'
import { t } from '../../i18n'
import { formatFileSize } from '../../repositoryAssets'
import { reorderVersionRows, type AssetVersionRow } from '../../mapPublishFlow'

/**
 * 版本排序表：一行 = 一个 Release 附件，**最新版在最上面**。
 *
 * 存储顺序仍是旧→新（`asset_versions` 契约），显示时整体反转一次；拖动与键盘
 * 移动都换算成存储下标再交回页面，组件自己不保存顺序。
 *
 * 拖动用 Pointer Events 而不是 HTML5 `draggable`：Windows 上 HTML5 拖拽走 OLE
 * 拖放，会触发 Tauri 的窗口拖放事件，整窗弹出「松开即可安装」的提示条，看起来
 * 像这个页面在接收安装包。
 */
const props = defineProps<{
  /** 这一行对应的附件名：规则在页面里，组件不重复算一遍。 */
  nameOf: (row: AssetVersionRow) => string
  disabled?: boolean
}>()

const rows = defineModel<AssetVersionRow[]>('rows', { required: true })
const emit = defineEmits<{ remove: [row: AssetVersionRow] }>()

/** 界面顺序：最新在最上。 */
const displayRows = computed(() => [...rows.value].reverse())

const draggingIndex = ref(-1)
const dropIndex = ref(-1)
/** 一行的高度（含间距），拖动时按位移换算跨了几行。 */
let stepHeight = 0
let startY = 0

function onPointerDown(event: PointerEvent, index: number) {
  if (props.disabled) return
  event.preventDefault()
  const handle = event.currentTarget as HTMLElement
  handle.setPointerCapture(event.pointerId)
  draggingIndex.value = index
  dropIndex.value = index
  startY = event.clientY
  const row = handle.closest('.version-row') as HTMLElement | null
  stepHeight = (row?.offsetHeight ?? 0) + 8
}

function onPointerMove(event: PointerEvent) {
  if (draggingIndex.value < 0 || stepHeight <= 0) return
  const offset = Math.round((event.clientY - startY) / stepHeight)
  const last = displayRows.value.length - 1
  dropIndex.value = Math.max(0, Math.min(last, draggingIndex.value + offset))
}

function onPointerUp(event: PointerEvent) {
  const from = draggingIndex.value
  const to = dropIndex.value
  draggingIndex.value = -1
  dropIndex.value = -1
  const handle = event.currentTarget as HTMLElement
  if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId)
  if (from < 0 || to < 0 || from === to) return
  rows.value = reorderVersionRows(rows.value, from, to)
}

/** Alt + ↑/↓：键盘也能调顺序，不强迫用鼠标拖。 */
function onKeydown(event: KeyboardEvent, index: number) {
  if (props.disabled || !event.altKey) return
  if (event.key !== 'ArrowUp' && event.key !== 'ArrowDown') return
  event.preventDefault()
  const to = index + (event.key === 'ArrowUp' ? -1 : 1)
  if (to < 0 || to >= displayRows.value.length) return
  rows.value = reorderVersionRows(rows.value, index, to)
}
</script>

<template>
  <ul class="version-list">
    <li v-for="(row, index) in displayRows" :key="index" class="version-row"
      :class="{
        dragging: draggingIndex === index,
        'drop-target': draggingIndex >= 0 && dropIndex === index && draggingIndex !== index,
      }">
      <button type="button" class="drag-handle" :title="t('mapPublish.versionMapDragHint')" :disabled="disabled"
        @pointerdown="onPointerDown($event, index)" @pointermove="onPointerMove" @pointerup="onPointerUp"
        @pointercancel="onPointerUp" @keydown="onKeydown($event, index)">
        <ReorderThreeOutline width="16" height="16" />
      </button>
      <span class="version-index">{{ index + 1 }}</span>
      <div class="version-main">
        <code class="mono version-asset">{{ nameOf(row) || row.mapsetVersion }}</code>
        <span class="muted">{{ formatFileSize(row.size) }}</span>
      </div>
      <div class="version-fields">
        <n-input v-model:value="row.mapsetVersion" size="tiny" :placeholder="t('mapPublish.versionMapMapset')"
          :disabled="disabled" />
        <n-input v-model:value="row.infoVersion" size="tiny" :placeholder="t('mapPublish.versionMapInfo')"
          :disabled="disabled" />
      </div>
      <n-tag v-if="index === 0" size="tiny" type="success" :bordered="false">
        {{ t('mapPublish.versionMapLatest') }}
      </n-tag>
      <n-button size="tiny" type="error" secondary :disabled="disabled || !nameOf(row)"
        @click="emit('remove', row)">
        {{ t('mapPublish.deleteAsset') }}
      </n-button>
    </li>
  </ul>
</template>

<style scoped>
.version-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.version-row {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 10px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  background: var(--oc2-tile);
}
.version-row.dragging { opacity: .5; }
.version-row.drop-target {
  border-color: var(--oc2-accent);
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 22%, transparent);
}

.drag-handle {
  display: flex;
  align-items: center;
  flex: 0 0 auto;
  padding: 0;
  border: 0;
  background: none;
  color: var(--oc2-text-3);
  cursor: grab;
  touch-action: none;
}
.drag-handle:active { cursor: grabbing; }
.drag-handle:disabled { cursor: default; opacity: .5; }

.version-index {
  flex: 0 0 auto;
  min-width: 16px;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  color: var(--oc2-text-3);
  text-align: right;
}

.version-main {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1 1 auto;
  min-width: 0;
}
.version-asset {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
}
.version-main .muted { flex: 0 0 auto; font-size: 12px; }

.version-fields {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}
.version-fields .n-input { width: 104px; }
</style>
