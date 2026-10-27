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
  <AppModal :show="show" :title="t('回收站')" max-width="min(860px, 90%)"
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
 * 弹窗内的列表需要有确定高度才能让空态居中：
 * AppModal 的 #footer 为空时不渲染，内容区就是唯一可分配空间。
 * 用 min-height 兜住「条目很少」的情况，避免弹窗缩成一条。
 */
.recycle-dialog-body {
  display: flex;
  flex-direction: column;
  min-height: min(420px, 52vh);
  max-height: min(620px, 68vh);
  overflow-y: auto;
}
</style>
