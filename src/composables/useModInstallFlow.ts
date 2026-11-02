/**
 * 安装流程的宿主协调器。
 *
 * 把「来源选择 → 预览 → 确认安装」三段串起来，两个页面（MOD 管理 /
 * MOD 安装）和仓库卡片共用同一份接线，避免每个入口各写一套状态机。
 *
 * 三段各自独立：
 *   InstallSourceCard       取得来源（本地 / 直链 / 拖放）
 *   ModInstallDialog        展示预览并确认
 *   useModInstallFlow       持有状态、串接两者、负责放弃时清理暂存
 *
 * 用法：
 *   const flow = useModInstallFlow()
 *   flow.openSource()                  // 打开「选择来源」
 *   flow.openPreview(somePreview)      // 直接展示一份已有预览（仓库资产）
 *   模板里挂 <InstallSourceCard v-model:show="flow.sourceOpen"
 *                             :busy="flow.sourceBusy" :pick-file="flow.pickSourceFile"
 *                             @select="flow.handleSourceSelect" />
 *         和 <ModInstallDialog v-model:show="flow.previewOpen" ... />
 */
import { ref } from 'vue'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { beginDownloadTask, endDownloadTask, failDownloadTask } from '../downloadTask'
import type { InstallSource, ModPreview } from '../types'

export function useModInstallFlow(onInstalled?: () => void | Promise<void>) {
  const message = useNotify()
  /** 「选择来源」面板是否打开。 */
  const sourceOpen = ref(false)
  /** 来源卡片正在解析上一条来源（下载 / 预览），期间显示转圈。 */
  const sourceBusy = ref(false)
  /** 「安装详情」面板是否打开。 */
  const previewOpen = ref(false)
  /** 当前展示的预览。 */
  const preview = ref<ModPreview | null>(null)

  /** 打开来源选择（本地文件 / 在线直链）。 */
  function openSource() {
    sourceOpen.value = true
  }

  /** 直接展示一份已解析好的预览（仓库资产走 preview_only 下载后得到）。 */
  function openPreview(next: ModPreview) {
    preview.value = next
    previewOpen.value = true
  }

  /**
   * 来源卡片给到的本地文件 / 直链：解析成预览后交给安装详情弹窗。
   *
   * 放在这里而不是两个页面各写一遍：MOD 安装页与 MOD 管理页的「本地选择 /
   * 直链 / 拖放」确认后要做的事完全一样（解析 → 预览 → 失败提示）。
   */
  async function handleSourceSelect(source: InstallSource) {
    if (sourceBusy.value) return
    sourceBusy.value = true
    /*
     * 直链要先把文件下载到临时目录才能解析，所以失败可能是下载失败
     * （首字节超时等），要能在下载卡片上原地重试。本地文件只读盘，
     * 失败与网络无关，重试只会得到同一个结果，不挂到下载卡片上。
     */
    const token = source.kind === 'url'
      ? beginDownloadTask({
        label: t('direct.onlineFile'),
        retry: () => { void handleSourceSelect(source) },
      })
      : null
    try {
      const next = await api.analyzeModSource(source.kind, source.value)
      if (token !== null) endDownloadTask(token)
      // 来源已经解析成功：先收起来源卡片，再打开安装详情
      sourceOpen.value = false
      openPreview(next)
    } catch (reason) {
      if (token !== null) failDownloadTask(token, reason)
      message.error(backendMessageOf(reason))
    } finally {
      sourceBusy.value = false
    }
  }

  /** 来源卡片的文件选择器：MOD 只认 DLL / ZIP。 */
  function pickSourceFile() {
    return api.chooseModFile()
  }

  /**
   * 放弃一份预览：删掉后端暂存，避免下载的包留在磁盘上。
   *
   * 由预览弹窗的 `discarded` 事件触发（取消按钮、叉掉、被新的预览替换）。
   */
  async function discardPreview(discarded: ModPreview) {
    if (preview.value?.preview_id === discarded.preview_id) preview.value = null
    await api.cancelModPreview(discarded.preview_id).catch(() => undefined)
  }

  /** 安装完成：清空状态并通知宿主刷新。 */
  async function handleInstalled() {
    preview.value = null
    await onInstalled?.()
  }

  return {
    sourceOpen, sourceBusy, previewOpen, preview,
    openSource, openPreview, handleSourceSelect, pickSourceFile,
    discardPreview, handleInstalled,
  }
}
