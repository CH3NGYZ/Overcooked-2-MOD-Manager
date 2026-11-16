<script setup lang="ts">
/** Full recycle bin view, opened from tools or the titlebar summary. */
import { NButton } from 'naive-ui'
import { RefreshOutline, TrashOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { useRecycleBin } from '../composables/useRecycleBin'
import { useRecycleBinActions } from '../composables/useRecycleBinActions'
import AppModal from './AppModal.vue'
import RecycleBinList from './RecycleBinList.vue'
import RecycleBinSummary from './RecycleBinSummary.vue'

defineProps<{ show: boolean }>()
const emit = defineEmits<{ (event: 'update:show', value: boolean): void }>()
const { count, loading, busy, acting, reload } = useRecycleBin()
const { confirmClearAll } = useRecycleBinActions()
</script>

<template>
  <!--
    fill：打开就占满允许的最大区域。
    回收站列表的条目数量在打开前未知，若随内容自适应，短列表会让弹窗
    缩成一条、长列表又要等渲染完才撑开。固定成最大尺寸，空态与滚动都在
    一个稳定的框里，不会在数据到达时跳变。
  -->
  <AppModal :show="show" :title="t('recycle.recycleBin')" fill max-width="min(860px, 90%)"
    @update:show="(value: boolean) => emit('update:show', value)">
    <template #header-extra>
      <div class="recycle-dialog-actions">
        <RecycleBinSummary />
        <n-button size="small" quaternary :loading="loading" :disabled="busy" :title="t('recycle.refreshNow')"
          :aria-label="t('recycle.refreshNow')" @click="reload">
          <template #icon><RefreshOutline width="15" height="15" /></template>
        </n-button>
        <n-button size="small" quaternary type="error" :title="t('recycle.emptyRecycleBin')"
          :aria-label="t('recycle.emptyRecycleBin')" :loading="acting?.kind === 'clear'"
          :disabled="busy || loading || !count" @click="confirmClearAll">
          <template #icon><TrashOutline width="15" height="15" /></template>
        </n-button>
      </div>
    </template>
    <div class="recycle-dialog-body">
      <RecycleBinList />
    </div>
  </AppModal>
</template>

<style scoped>
.recycle-dialog-actions { display: flex; align-items: center; gap: 8px; }
/*
 * 弹窗内容区：吃满 AppModal 分配的高度，滚动交给列表自己。
 *
 * fill 已经给出确定高度，所以这里只要 flex: 1 + min-height: 0 —— 不需要
 * （也不该）再写 min-height / max-height：那会和 fill 的尺寸打架，
 * 出现「弹窗按上限撑开、内容却按另一个上限收缩」的双重约束。
 */
.recycle-dialog-body {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
}
</style>
