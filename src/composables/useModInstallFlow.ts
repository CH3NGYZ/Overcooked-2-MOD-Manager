/**
 * 安装流程的宿主协调器。
 *
 * 把「来源选择 → 同身份冲突检查 → 预览 → 确认安装」串起来，两个页面
 * （MOD 管理 / MOD 安装）和仓库卡片共用同一份接线，避免每个入口各写一套状态机。
 *
 * 各段各自独立：
 *   InstallSourceCard       取得来源（本地 / 直链 / 拖放）
 *   InstallCheckDialog      装之前先把同身份的旧装项卸掉（与地图导入共享）
 *   ModInstallDialog        展示预览并确认
 *   useModInstallFlow       持有状态、串接它们、负责放弃时清理暂存
 *
 * 用法：
 *   const flow = useModInstallFlow(() => load())
 *   flow.openSource()                  // 打开「选择来源」
 *   flow.openPreview(somePreview)      // 直接展示一份已有预览（仓库资产）
 *   模板里挂 <InstallSourceCard v-model:show="flow.sourceOpen"
 *                             :busy="flow.sourceBusy" :pick-file="flow.pickSourceFile"
 *                             @select="flow.handleSourceSelect" />
 *          <InstallCheckDialog v-model:show="flow.checkOpen" :model="flow.checkModel"
 *                              :execute="flow.executeCheck" @finished="flow.finishCheck" />
 *          <ModInstallDialog v-model:show="flow.previewOpen" ... />
 */
import { ref } from 'vue'
import { useGameOperationGate } from './useGameOperationGate'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { beginDownloadTask, endDownloadTask, failDownloadTask } from '../downloadTask'
import { uninstallModConflict } from '../installConflicts'
import { conflictCheckModel } from '../installCheck'
import { modCheckTarget, needsInstallCheck, withConflictsResolved } from '../modInstallCheck'
import { useInstallCheck } from './useInstallCheck'
import type { InstallCheckRow } from '../installCheck'
import type { InstallConflict, InstallSource, ModPreview } from '../types'

/**
 * 把检查行翻译成一次卸载。
 *
 * 行的下标与 `conflicts` 数组一一对应（`conflictCheckModel` 就是按下标生成的），
 * 所以用 `row.index` 取回原始条目。失败直接抛给弹窗：它负责把这一行标 ✕、
 * 把原因挂在行下方，再点「全部处理」只重试还没成功的那几行 —— 已进回收站的
 * 不能重复卸载，否则第二次必报「找不到」。
 */
async function executeConflictUninstall(
  conflicts: readonly InstallConflict[],
  row: InstallCheckRow,
): Promise<void> {
  const conflict = conflicts[row.index]
  if (!conflict) throw new Error(t('modsOnline.theModToUninstallWasNotFound'))
  await uninstallModConflict(conflict)
}

/**
 * @param onChanged 磁盘内容可能已变时通知宿主重扫（安装完成、或检查阶段
 *   卸过旧装项后放弃安装）。两个页面传的都是自己的 `load()`。
 */
export function useModInstallFlow(onChanged?: () => void | Promise<void>) {
  const message = useNotify()
  const { blockModOperation } = useGameOperationGate()
  /** 「选择来源」面板是否打开。 */
  const sourceOpen = ref(false)
  /** 来源卡片正在解析上一条来源（下载 / 预览），期间显示转圈。 */
  const sourceBusy = ref(false)
  /** 「安装详情」面板是否打开。 */
  const previewOpen = ref(false)
  /** 当前展示的预览。 */
  const preview = ref<ModPreview | null>(null)
  /** 同身份冲突的前置检查（共享弹窗的宿主接线）。 */
  const check = useInstallCheck()
  /**
   * 已经在前置检查里处理过冲突的预览 id。
   *
   * 这类预览即使最后被用户放弃，磁盘也已经变了（旧装项进了回收站），宿主必须
   * 重扫一次；只认 id 而不是一个布尔，才不会被随后另一份预览的状态串掉。
   */
  let resolvedPreviewId: string | null = null

  /** 打开来源选择（本地文件 / 在线直链）。 */
  function openSource() {
    if (blockModOperation()) return
    sourceOpen.value = true
  }

  /**
   * 展示一份已解析好的预览（仓库资产走 preview_only 下载后得到）。
   *
   * 有同身份冲突时先进共享的「安装前检查」：卸掉本地已有副本、卸干净才进入
   * 安装详情。冲突只有在**解析之后**才存在 —— 后端要从包内容读出 GUID 与名字
   * 才能和已装项比对 —— 所以这道门只能放在预览这一步；五个安装入口（两页的
   * 来源卡片与拖放、跨页跳转、仓库卡片与「更新」）全部汇到本函数。
   *
   * 这里**不 await** 检查结论：仓库卡片在 `openPreview` 返回后就要恢复 loading，
   * 不能让整页在用户读弹窗时一直转圈。
   */
  function openPreview(next: ModPreview) {
    if (blockModOperation()) {
      void discardPreview(next)
      return
    }
    if (!needsInstallCheck(next)) {
      preview.value = next
      previewOpen.value = true
      return
    }
    void runConflictCheck(next).catch((reason: unknown) => {
      message.error(backendMessageOf(reason))
    })
  }

  /** 同身份冲突：卸掉本地已有副本，全部成功才进入安装详情。 */
  async function runConflictCheck(next: ModPreview) {
    const ready = await check.run(
      conflictCheckModel(next.conflicts, { targetLabel: modCheckTarget(next) }),
      (row) => {
        if (blockModOperation()) throw new Error('game.running')
        return executeConflictUninstall(next.conflicts, row)
      },
    )
    if (ready) {
      /*
       * 冲突已经处理完：清空 `conflicts` 再交给安装详情 —— 它的警示块与
       * 「卸载并安装」按钮是为「还没卸」准备的，此时再说一遍只会让用户
       * 以为要卸两次。后端 `confirm_mod_install` 不复查冲突，清空不影响安装。
       */
      resolvedPreviewId = next.preview_id
      preview.value = withConflictsResolved(next)
      previewOpen.value = true
      return
    }
    // 取消或失败：后端暂存要清掉；磁盘可能已被动过（卸了几项才取消），让宿主重扫
    await discardPreview(next)
    await onChanged?.()
  }

  /**
   * 来源卡片给到的本地文件 / 直链：解析成预览后交给安装详情弹窗。
   *
   * 放在这里而不是两个页面各写一遍：MOD 安装页与 MOD 管理页的「本地选择 /
   * 直链 / 拖放」确认后要做的事完全一样（解析 → 预览 → 失败提示）。
   */
  async function handleSourceSelect(source: InstallSource) {
    if (blockModOperation()) return
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
    const changed = resolvedPreviewId === discarded.preview_id
    if (changed) resolvedPreviewId = null
    if (preview.value?.preview_id === discarded.preview_id) preview.value = null
    await api.cancelModPreview(discarded.preview_id).catch(() => undefined)
    // 检查阶段已经卸过旧装项：放弃安装也不能把列表留在旧状态
    if (changed) await onChanged?.()
  }

  /** 安装完成：清空状态并通知宿主刷新。 */
  async function handleInstalled() {
    resolvedPreviewId = null
    preview.value = null
    await onChanged?.()
  }

  return {
    sourceOpen, sourceBusy, previewOpen, preview,
    openSource, openPreview, handleSourceSelect, pickSourceFile,
    discardPreview, handleInstalled,
    checkOpen: check.open,
    checkModel: check.model,
    executeCheck: check.execute,
    finishCheck: check.finish,
  }
}
