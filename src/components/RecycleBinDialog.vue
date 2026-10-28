<script setup lang="ts">
/**
 * 回收站弹窗 —— 回收站的唯一入口（工具页「回收站设置」→「打开回收站」）。
 *
 * 只做外壳：标题、刷新与清空按钮、尺寸上限，内容全部交给 `RecycleBinList`。
 * 回收站既不占主导航也没有独立页面：它是低频维护功能，不值得一个常驻入口。
 */
import { ref } from 'vue'
import { NButton, useDialog } from 'naive-ui'
import { RefreshOutline, TrashOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import AppModal from './AppModal.vue'
import RecycleBinList from './RecycleBinList.vue'

defineProps<{ show: boolean }>()
const emit = defineEmits<{ (event: 'update:show', value: boolean): void }>()

const dialog = useDialog()
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

/**
 * 清空回收站。
 *
 * 这是全应用破坏性最强的单个动作：一次性移走所有 MOD 的**唯一副本**
 * （暂存文件与清单快照），删完不能恢复。所以不放进任何菜单，而用
 * 标题栏图标 + 明确的二次确认，并在问句里点名条数。
 */
function confirmClearAll() {
  const count = list.value?.entryCount ?? 0
  if (!count) return
  dialog.error({
    title: t('清空回收站'),
    content: t('将永久删除回收站中的全部 {count} 条记录及其文件，无法恢复。确定清空？', { count }),
    positiveText: t('全部彻底删除'),
    negativeText: t('取消'),
    onPositiveClick() {
      void list.value?.clearAll()
    },
  })
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
      <n-button size="small" quaternary type="error" :title="t('清空回收站')"
        :disabled="!list?.entryCount" @click="confirmClearAll">
        <template #icon><TrashOutline width="15" height="15" /></template>
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
