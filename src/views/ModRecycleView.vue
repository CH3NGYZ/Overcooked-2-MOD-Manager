<script setup lang="ts">
/**
 * MOD 回收站页面。
 *
 * 管理器删除的 MOD 暂存在用户数据根下的 `recycle-bin/`，在这里可以恢复到
 * 原位置或彻底删除。列表本身由 `components/RecycleBinList.vue` 承担 —— 工具页
 * 的弹窗入口用的是同一个组件，两处行为与文案必须一致。
 *
 * 本文件只负责页面外壳：刷新按钮放进页头、安排高度链。
 */
import { ref } from 'vue'
import { NButton } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { useDashboard } from '../composables/useDashboardStore'
import RecycleBinList from '../components/RecycleBinList.vue'

const { ensureLoaded } = useDashboard()
const list = ref<InstanceType<typeof RecycleBinList> | null>(null)
const refreshing = ref(false)

void ensureLoaded()

/** 刷新列表（只读操作，游戏运行时也可用）。 */
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
  <Teleport to="#page-header-actions" defer>
    <n-button size="small" secondary :loading="refreshing" @click="reload">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('立即刷新') }}
    </n-button>
  </Teleport>

  <RecycleBinList ref="list" />
</template>
