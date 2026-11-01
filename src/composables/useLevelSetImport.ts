/**
 * 地图 ZIP 的「暂存 → 确认 → 导入」—— 唯一实现。
 *
 * 两个页面共用：地图下载页的「安装地图」来源卡片（本地选择 / 拖入窗口 / 直链），
 * 以及地图管理页的整窗拖放。写两份的代价不是多几十行，而是确认界面、覆盖提示、
 * 暂存清理与「装完重读」的时机各漂各的 —— 用户会在两页看到不同的东西。
 *
 * 确认界面不再是自拼的 `dialog.warning`：把每个地图集当成一个 MOD、随包插件
 * 当成它下面那张卡，交给和 MOD 安装共用的 `InstallDetailDialog` 展示。所以
 * 这里的职责只剩状态与顺序：暂存、打开详情、等用户表态、导入、清理暂存。
 *
 * 后端 `stage_map_source` 先把来源统一成一份可读 ZIP（本地文件原地引用、
 * 直链下载到 cache 并标记 `staged`），识别结果与 `inspect_level_set_zip` 同构，
 * 所以「确认导入」只需要一份实现。
 *
 * 调用方只负责两件事：把来源交进来；导入成功后重读自己的关卡集列表。
 */
import { computed, ref, watch } from 'vue'
import { t } from '../i18n'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { levelSetImportDetail } from '../installDetail'
import { backendMessageOf } from '../locale/backend'
import type { InstallDetailModel } from '../installDetail'
import type { InstallSource, ZipLevelSet } from '../types'

/** 正在等用户确认的那一批：ZIP 位置 + 识别出的关卡集。 */
interface PendingBatch {
  path: string
  sets: ZipLevelSet[]
}

export function useLevelSetImport(onImported?: () => void | Promise<void>) {
  const message = useNotify()
  /** 正在处理一份来源：暂存 / 识别 / 等确认 / 导入，期间入口按钮转圈并禁用。 */
  const importing = ref(false)
  const pending = ref<PendingBatch | null>(null)
  /** 详情弹窗是否打开。 */
  const previewOpen = ref(false)

  /**
   * 详情面板模型：一个地图集当一个 MOD，随包插件缩进挂在它下面。
   *
   * 用 computed 而不是打开时算一份快照 —— 弹窗开着时切语言，标题、字段名与
   * 计数摘要要跟着变（`t()` 在 computed 里被执行，locale 变化会被追踪）。
   */
  const detail = computed<InstallDetailModel | null>(() =>
    pending.value ? levelSetImportDetail(pending.value.path, pending.value.sets) : null)

  /** 「立即安装」与「取消」的结论，交回 `importSource` 里那个 await。 */
  let settleApproval: ((approved: boolean) => void) | null = null

  function resolveApproval(approved: boolean) {
    const settle = settleApproval
    settleApproval = null
    settle?.(approved)
  }

  /** 打开详情弹窗，等用户点「立即安装」或放弃。 */
  function askApproval(path: string, sets: ZipLevelSet[]) {
    pending.value = { path, sets }
    previewOpen.value = true
    return new Promise<boolean>((resolve) => { settleApproval = resolve })
  }

  /**
   * 弹窗被任何方式收起（叉掉、Esc、点遮罩、宿主自己关）都算放弃。
   * 不接这一条的话，`importSource` 会永远挂在那个 await 上，`importing`
   * 一直是 true，页面的安装按钮从此转圈。
   */
  watch(previewOpen, (open) => { if (!open) resolveApproval(false) })

  function closePreview() {
    previewOpen.value = false
    pending.value = null
  }

  /** 「立即安装」：只表态，真正的导入仍由 `importSource` 那条链负责。 */
  function confirmImport() {
    resolveApproval(true)
  }

  /** 「取消」：收起弹窗并放弃这一批（暂存由 `importSource` 的 finally 清理）。 */
  function discardImport() {
    previewOpen.value = false
    resolveApproval(false)
  }

  /**
   * 导入一个来源（本地路径或直链）。
   *
   * `onStaged` 在「暂存并识别成功」之后、详情弹窗之前调用：来源卡片据此收起
   * 自己（弹窗才是接下来的主角），暂存失败时卡片留在原地显示错误。
   * 失败已就地提示，不向调用方抛。
   */
  async function importSource(source: InstallSource, options?: { onStaged?: () => void }) {
    if (importing.value) return
    importing.value = true
    /**
     * 后端下载到 cache 的临时 ZIP：不论导入成功、失败还是用户取消，结束都要
     * 清掉，否则每装一次地图就在 cache 里留一份 ZIP。`null` 表示用户自己的
     * 本地文件（`staged` 为假），绝不能删。
     */
    let stagedFile: string | null = null
    try {
      const staged = await api.stageMapSource(source.kind, source.value)
      if (staged.staged) stagedFile = staged.path
      options?.onStaged?.()
      if (!staged.sets.length) {
        message.warning(t('levelSet.zipWithoutInfo'))
        return
      }
      if (!(await askApproval(staged.path, staged.sets))) return
      /*
       * 用户表态后先收起弹窗再导入：写盘期间进度由页面按钮转圈，弹窗留着
       * 会一直挂着「取消」按钮，看着像还能反悔。
       */
      closePreview()
      const selected = staged.sets.map((set) => set.dir_name)
      const result = await api.importLevelSets(staged.path, selected)
      message.success(t('levelSet.imported', { count: result.installed.length }))
      await onImported?.()
    } catch (reason) {
      message.error(backendMessageOf(reason))
    } finally {
      closePreview()
      if (stagedFile) await api.discardMapSource(stagedFile).catch(() => undefined)
      importing.value = false
    }
  }

  return { importing, importSource, previewOpen, detail, confirmImport, discardImport }
}
