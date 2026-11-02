<script setup lang="ts">
/**
 * 「查看安装详情」弹窗 —— 所有 MOD 安装入口共用的唯一预览界面。
 *
 * 分工（这一层只剩两件事）：
 *   1. 把调用方传入的 `ModPreview` 翻译成共用详情模型（`installDetail.ts`）
 *   2. 用户点「立即安装」调 confirm_mod_install；取消/关闭时通知调用方放弃
 * 卡片长什么样、要展示哪九项字段，全在 `InstallDetailDialog` 里 —— 那里同时
 * 承担地图导入的展示，两个域不再各写一套。
 *
 * 它**不负责**获取预览：本地选文件、直链下载、仓库资产下载都在调用方，
 * 因为那三种来源的下载方式（文件选择器 / 直连 / 多源竞速）差异很大，
 * 塞进弹窗会让这里变成三种流程的交叉分支。
 *
 * 调用方把预览通过 `preview` prop 传进来，弹窗只负责渲染与确认，
 * 不持有预览副本 —— 否则调用方与弹窗各存一份，只有一方被更新时
 * 弹窗会打开但内容为空。
 */
import { computed, ref, watch } from 'vue'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { celebrateInstall } from '../celebration'
import { modPreviewDetail } from '../installDetail'
import { uninstallConflicts, uninstallModConflict } from '../installConflicts'
import { t } from '../i18n'
import InstallDetailDialog from './InstallDetailDialog.vue'
import type { InstallConflict, ModPreview } from '../types'
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
const installing = ref(false)

/** 预览 → 详情模型（含「有没有可安装的内容」这一判定，空包只能取消）。 */
const detail = computed(() => (props.preview ? modPreviewDetail(props.preview) : null))

/**
 * 卸载失败后还没处理完的冲突。
 *
 * 弹窗在失败时保持打开，用户处理完外部原因（关游戏、手动删掉占用的那份）
 * 会直接在同一个弹窗上再点一次；那时只能重试剩下的项，已经卸载的不能再卸，
 * 否则第二次必然报「找不到」。预览换了（`preview_id` 变）就重置。
 */
const pendingConflicts = ref<InstallConflict[] | null>(null)
const conflictsToUninstall = computed(() => pendingConflicts.value ?? props.preview?.conflicts ?? [])
watch(() => props.preview?.preview_id, () => { pendingConflicts.value = null })

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

/** 用户确认安装。 */
async function confirm() {
  const current = props.preview
  if (!current) return
  installing.value = true
  try {
    /*
     * 冲突项先卸载（进回收站）再安装：同一个 DLL 已存在时后端预览会把它
     * 列进 conflicts。卸载失败就中止 —— 此时还没调用 confirm，弹窗保持
     * 打开让用户看清是哪一项没卸掉，不能关掉窗口假装安装完成。
     */
    if (conflictsToUninstall.value.length > 0) {
      const uninstall = await uninstallConflicts(conflictsToUninstall.value, uninstallModConflict)
      if (!uninstall.ok) {
        // 只剩没卸掉的项：重试不会把已进回收站的那几份再删一次
        pendingConflicts.value = uninstall.remaining
        message.error(t('tools.installConflictFailed', {
          name: uninstall.failed?.name ?? '',
          reason: uninstall.reason ?? '',
        }))
        return
      }
      pendingConflicts.value = []
    }
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
