<script setup lang="ts">
/**
 * 「查看安装详情」弹窗 —— 所有 MOD 安装入口共用的唯一预览界面。
 *
 * 分工（这一层只剩两件事）：
 *   1. 把调用方传入的 `ModPreview` 翻译成共用详情模型（`installDetail.ts`）
 *   2. 用户点「立即安装」调 confirm_mod_install；取消/关闭时通知调用方放弃
 * 卡片长什么样、要展示哪几项字段，全在 `InstallDetailDialog` 里 —— 那里同时
 * 承担地图导入的展示，两个域不再各写一套。
 *
 * 它**不负责**获取预览：本地选文件、直链下载、仓库资产下载都在调用方，
 * 因为那三种来源的下载方式（文件选择器 / 直连 / 多源竞速）差异很大，
 * 塞进弹窗会让这里变成三种流程的交叉分支。
 *
 * 它也**不负责卸载同身份冲突**：那一步发生在打开本弹窗之前（`useModInstallFlow`
 * 的前置检查，与地图导入共用 `InstallCheckDialog`），卸干净之后 `conflicts`
 * 已被清空，所以「立即安装」就是一次纯安装。把卸载留在这里会让同一次安装有
 * 两个卸载入口（检查弹窗卸一遍、确认时再卸一遍），而且卸载失败时弹窗内容
 * 已经说不清「哪一项是旧的、哪一项是要装的」。
 *
 * 调用方把预览通过 `preview` prop 传进来，弹窗只负责渲染与确认，
 * 不持有预览副本 —— 否则调用方与弹窗各存一份，只有一方被更新时
 * 弹窗会打开但内容为空。
 */
import { computed, ref } from 'vue'
import { useGameOperationGate } from '../composables/useGameOperationGate'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { celebrateInstall } from '../celebration'
import { modPreviewDetail } from '../installDetail'
import InstallDetailDialog from './InstallDetailDialog.vue'
import type { ModPreview } from '../types'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'

const props = defineProps<{ show: boolean; preview: ModPreview | null }>()
const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  /**
   * 安装成功。
   *
   * 带上刚装的那份预览：宿主据此知道「装的是哪个包」，进而在刷新后
   * 检查它有没有缺依赖。不带参数的话宿主无法区分「刚装的是这个」
   * 与「以前装的另一个也缺依赖」。
   */
  (event: 'installed', preview: ModPreview | null): void
  /** 用户放弃这份预览（取消按钮、叉掉），调用方据此清理暂存。 */
  (event: 'discarded', preview: ModPreview): void
}>()

const message = useNotify()
const { blockModOperation } = useGameOperationGate()
const installing = ref(false)

/** 预览 → 详情模型（含「有没有可安装的内容」这一判定，空包只能取消）。 */
const detail = computed(() => (props.preview ? modPreviewDetail(props.preview) : null))

/** 这次关闭是否由「用户放弃」触发（安装成功后不该再清理暂存）。 */
const discarding = ref(false)
/** 安装已成功、等离场动画结束后再通知宿主（否则内容先消失、看不到渐隐）。 */
const installCompleted = ref(false)
/** 刚装成功的那份预览：离场动画结束时随 `installed` 一起交给宿主。 */
const installedPreview = ref<ModPreview | null>(null)

/** 用户放弃：立刻收起，动画结束后再通知调用方清理。 */
function requestDiscard() {
  discarding.value = true
  emit('update:show', false)
}

/** 弹窗自己发起的开关（叉掉、Esc、点遮罩）：关闭一律走「放弃」语义。 */
function handleShow(value: boolean) {
  if (value) emit('update:show', true)
  else requestDiscard()
}

/**
 * 离场动画结束：此时内容已经淡出，可以安全清空并通知调用方。
 * Naive 的 modal 用 fade-in-scale-up-transition（缩小 + 渐隐）。
 */
function handleAfterLeave() {
  if (discarding.value) {
    discarding.value = false
    const current = props.preview
    if (current) emit('discarded', current)
  }
  if (installCompleted.value) {
    installCompleted.value = false
    const done = installedPreview.value
    installedPreview.value = null
    emit('installed', done)
  }
}

/** 用户确认安装。同身份冲突已在打开本弹窗之前处理完，这里只装。 */
async function confirm() {
  if (blockModOperation()) return
  const current = props.preview
  if (!current) return
  installing.value = true
  try {
    const result = await api.confirmModInstall(current.preview_id)
    if (!result.ok) {
      message.error(translateBackendMessage(result.message))
      return
    }
    message.success(translateBackendMessage(result.message))
    // 安装成功是低频、高价值时刻：放一次短促彩带（内部已尊重 reduced-motion）
    void celebrateInstall()
    // 后端已在 confirm 里删掉暂存；收起面板，动画结束后再通知宿主刷新
    installCompleted.value = true
    installedPreview.value = current
    emit('update:show', false)
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    installing.value = false
  }
}
</script>

<template>
  <InstallDetailDialog :show="show" :model="detail" :busy="installing" :cancel-disabled="installing"
    @update:show="handleShow" @discard="requestDiscard" @confirm="confirm"
    @after-leave="handleAfterLeave" />
</template>
