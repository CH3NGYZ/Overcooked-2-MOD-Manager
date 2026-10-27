<script setup lang="ts">
/**
 * 回收站弹窗（工具页入口用）。
 *
 * 只做外壳：标题、刷新按钮、尺寸上限，内容全部交给 `RecycleBinList`。
 * 页面版 `views/ModRecycleView.vue` 用的是同一个列表组件，所以两处
 * 的条目样式、二次确认和失败提示完全一致。
 */
import { ref } from 'vue'
import { NButton } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import AppModal from './AppModal.vue'
import RecycleBinList from './RecycleBinList.vue'

defineProps<{ show: boolean }>()
const emit = defineEmits<{ (event: 'update:show', value: boolean): void }>()

const list = ref<InstanceType<typeof RecycleBinList> | null>(null)
const refreshing = ref(false)

/** 重新读取：列表组件自持状态，这里只负责把「刷新中」反馈到按钮上。 */
async function reload() {
  if (refreshing.value) return
  refreshing.value = true
  try {
    await list.value?.reload()
  } finally {
    refreshing.value = false
  }
}
</script>

<template>
  <!--
    fill：打开就占满允许的最大区域。
    回收站列表的条目数量在打开前未知，若随内容自适应，短列表会让弹窗
    缩成一条、长列表又要等渲染完才撑开。固定成最大尺寸，空态与滚动都在
    一个稳定的框里，不会在数据到达时跳变。
  -->
  <AppModal :show="show" :title="t('回收站')" fill max-width="min(860px, 90%)"
    @update:show="(value: boolean) => emit('update:show', value)">
    <template #header-extra>
      <n-button size="small" quaternary :loading="refreshing" :title="t('立即刷新')" @click="reload">
        <template #icon><RefreshOutline width="15" height="15" /></template>
      </n-button>
    </template>
    <div class="recycle-dialog-body">
      <RecycleBinList ref="list" />
    </div>
  </AppModal>
</template>

<style scoped>
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
