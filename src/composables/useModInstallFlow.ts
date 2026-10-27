/**
 * 安装流程的宿主协调器。
 *
 * 把「来源选择 → 预览 → 确认安装」三段串起来，两个页面（MOD 管理 /
 * MOD 安装）和仓库卡片共用同一份接线，避免每个入口各写一套状态机。
 *
 * 三段各自独立：
 *   ModInstallSourceDialog  取得预览（本地 / 直链 / 拖放）
 *   ModInstallDialog        展示预览并确认
 *   useModInstallFlow       持有状态、串接两者、负责放弃时清理暂存
 *
 * 用法：
 *   const flow = useModInstallFlow()
 *   flow.openSource()                  // 打开「选择来源」
 *   flow.openPreview(somePreview)      // 直接展示一份已有预览（仓库资产）
 *   模板里挂 <ModInstallSourceDialog v-model:show="flow.sourceOpen" ... />
 *         和 <ModInstallDialog v-model:show="flow.previewOpen" ... />
 */
import { ref } from 'vue'
import { api } from '../services/tauri'
import type { ModPreview } from '../types'

export function useModInstallFlow(onInstalled?: () => void | Promise<void>) {
  /** 「选择来源」面板是否打开。 */
  const sourceOpen = ref(false)
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
    sourceOpen, previewOpen, preview,
    openSource, openPreview, discardPreview, handleInstalled,
  }
}
