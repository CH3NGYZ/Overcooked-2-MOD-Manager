<script setup lang="ts">
import { computed, ref } from 'vue'
import { NButton, NInput, NTable, NTag } from 'naive-ui'
import { ReorderThreeOutline } from '@vicons/ionicons5'
import { t } from '../../i18n'
import { formatFileSize } from '../../repositoryAssets'
import { reorderVersionRows, type AssetVersionRow } from '../../mapPublishFlow'

const props = defineProps<{
  nameOf: (row: AssetVersionRow) => string
  disabled?: boolean
  removeDisabled?: boolean
}>()
const rows = defineModel<AssetVersionRow[]>('rows', { required: true })
const emit = defineEmits<{ remove: [row: AssetVersionRow]; relations: [row: AssetVersionRow] }>()
// 存储仍为旧→新，只有显示顺序反转，拖动和键盘共用纯函数换算。
const displayRows = computed(() => [...rows.value].reverse())
const draggingIndex = ref(-1)
const dropIndex = ref(-1)
let rowCenters: number[] = []
function onPointerDown(event: PointerEvent, index: number) {
  if (props.disabled || event.button !== 0) return
  event.preventDefault()
  const handle = event.currentTarget as HTMLElement
  handle.setPointerCapture(event.pointerId)
  draggingIndex.value = index
  dropIndex.value = index
  // 文件名可换行，不能按统一行高估算落点。
  rowCenters = [...(handle.closest('tbody')?.querySelectorAll('tr') ?? [])].map(row => {
    const rect = row.getBoundingClientRect()
    return rect.top + rect.height / 2
  })
}
function onPointerMove(event: PointerEvent) {
  if (draggingIndex.value < 0 || !rowCenters.length) return
  dropIndex.value = rowCenters.reduce((nearest, center, index) =>
    Math.abs(center - event.clientY) < Math.abs(rowCenters[nearest]! - event.clientY) ? index : nearest, 0)
}
function endDrag(event: PointerEvent, commit: boolean) {
  const from = draggingIndex.value
  const to = dropIndex.value
  draggingIndex.value = dropIndex.value = -1
  rowCenters = []
  const handle = event.currentTarget as HTMLElement
  if (handle.hasPointerCapture(event.pointerId)) handle.releasePointerCapture(event.pointerId)
  if (commit && !props.disabled && from >= 0 && to >= 0 && from !== to) rows.value = reorderVersionRows(rows.value, from, to)
}
function onKeydown(event: KeyboardEvent, index: number) {
  if (props.disabled || !event.altKey || !['ArrowUp', 'ArrowDown'].includes(event.key)) return
  event.preventDefault()
  const to = index + (event.key === 'ArrowUp' ? -1 : 1)
  if (to >= 0 && to < displayRows.value.length) rows.value = reorderVersionRows(rows.value, index, to)
}
</script>
<template>
  <div class="publish-table-scroll">
    <n-table class="metadata-table version-table" size="small" :single-line="false" :aria-label="t('mapPublish.versionOrderSection')">
      <thead><tr>
        <th scope="col">{{ t('mapPublish.orderColumn') }}</th>
        <th scope="col">{{ t('mapPublish.assetColumn') }}</th>
        <th scope="col">{{ t('mapPublish.versionMapMapset') }}</th>
        <th scope="col">{{ t('mapPublish.versionMapInfo') }}</th>
        <th scope="col">{{ t('mapPublish.sizeColumn') }}</th>
        <th scope="col">{{ t('mapPublish.statusColumn') }}</th>
        <th scope="col">{{ t('mapPublish.actionsColumn') }}</th>
      </tr></thead>
      <tbody>
        <tr v-for="(row, index) in displayRows" :key="row.mapsetVersion" class="version-row"
          :class="{ dragging: draggingIndex === index, 'drop-target': draggingIndex >= 0 && dropIndex === index && draggingIndex !== index }">
          <td><button type="button" class="drag-handle" :title="t('mapPublish.versionMapDragHint')"
            :aria-label="`${t('mapPublish.versionMapDragHint')} · ${nameOf(row) || row.mapsetVersion}`" :disabled="disabled"
            @pointerdown="onPointerDown($event, index)" @pointermove="onPointerMove" @pointerup="endDrag($event, true)"
            @pointercancel="endDrag($event, false)" @keydown="onKeydown($event, index)">
            <ReorderThreeOutline width="16" height="16" /><span>{{ index + 1 }}</span>
          </button></td>
          <th scope="row" class="version-asset mono">{{ nameOf(row) || row.mapsetVersion }}</th>
          <td class="mono">{{ row.mapsetVersion }}</td>
          <td><n-input v-model:value="row.infoVersion" size="small" :input-props="{ 'aria-label': t('mapPublish.versionMapInfo') }" :disabled="disabled" /></td>
          <td class="mono">{{ formatFileSize(row.size) }}</td>
          <td><n-tag v-if="index === 0" size="small" type="success" :bordered="false">{{ t('mapPublish.versionMapLatest') }}</n-tag><span v-else class="muted">—</span></td>
          <td><div class="version-actions">
            <n-button size="small" secondary :disabled="disabled" @click="emit('relations', row)">{{ t('mapPublish.assetRelations') }}</n-button>
            <n-button size="small" type="error" secondary :disabled="disabled || removeDisabled || !nameOf(row)" @click="emit('remove', row)">{{ t('common.action.delete') }}</n-button>
          </div></td>
        </tr>
      </tbody>
    </n-table>
  </div>
</template>
