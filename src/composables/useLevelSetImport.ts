/**
 * 地图 ZIP 的「暂存 → 确认 → 导入」—— 唯一实现。
 *
 * 两个页面共用：地图下载页的「安装地图」来源卡片（本地选择 / 拖入窗口 / 直链），
 * 以及地图管理页的整窗拖放。写两份的代价不是多几十行，而是确认界面、覆盖提示、
 * 暂存清理与成功提示各漂各的 —— 用户会在两页看到不同的东西。
 *
 * 地图集与随包插件交给共享整链方案处理：先展示详情与依赖影响，再统一确认、
 * 准备和执行。这里负责来源暂存、方案调用和清理；成功事件由 App 统一回读。
 *
 * 后端 `stage_map_source` 先把来源统一成一份可读 ZIP（本地文件原地引用、
 * 直链下载到 cache 并标记 `staged`），识别结果就是 `level_sets::
 * inspect_zip_with_progress` 的产物，所以「确认导入」只需要一份实现。
 *
 * 调用方传入来源，两页直接读共享关卡集列表。
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { pickI18n, t } from '../i18n'
import { api, watchLevelSetZipProgress } from '../services/tauri'
import { useNotify } from '../useNotify'
import { useModChanges } from '../modChanges/store'
import { levelSetImportDetail } from '../installDetail'
import { backendMessageOf } from '../locale/backend'
import { cachedInstallRelease } from '../repositoryAssets'
import { beginDownloadTask, endDownloadTask, failDownloadTask } from '../downloadTask'
import { useMapSources } from './useMapSources'
import type { InstallDetailModel } from '../installDetail'
import type {
  InstallSource, LevelSetZipProgress, StagedMapSource, ZipLevelSet,
} from '../types'

/** 正在等用户确认的那一批：ZIP 位置 + 识别出的关卡集。 */
interface PendingBatch {
  path: string
  sets: ZipLevelSet[]
  previousVersions: Record<string, string[]>
}

export function useLevelSetImport() {
  const message = useNotify()
  const changes = useModChanges()
  const { maps } = useMapSources()
  /** 正在处理一份来源：暂存 / 识别 / 等确认 / 导入，期间入口按钮转圈并禁用。 */
  const importing = ref(false)
  const pending = ref<PendingBatch | null>(null)
  /** 详情弹窗是否打开。 */
  const previewOpen = ref(false)

  /**
   * 来源 ZIP 的识别 / 导入进度（后端 `level-set-zip-progress`）。
   *
   * 三个入口（本地拖入、直链下载、在线 Release）都走后端同一条识别链，所以
   * 进度共用一份：`info` 阶段说「正在读取哪个 info」，`install` 阶段说
   * 「正在写第几张」。`null` = 当前没有可展示的进度（等待确认、或已经结束）。
   */
  const zipProgress = ref<LevelSetZipProgress | null>(null)
  /**
   * 进度文案；空串表示不显示。
   *
   * `done` 是**已完成数**：正在读的这一份还没算进去、正在写的这一张也刚开始，
   * 所以文案里的序号是 `done + 1`；最后一份夹到 `total`，不出现「14/13」。
   */
  const zipProgressText = computed(() => {
    const progress = zipProgress.value
    if (!progress) return ''
    const position = Math.min(progress.done + 1, progress.total)
    return t(
      progress.phase === 'install' ? 'levelSet.zipInstalling' : 'levelSet.zipReading',
      { name: progress.current, done: position, total: progress.total },
    )
  })
  /** 事件解绑函数：监听器不随页面卸载自动消失，必须自己收。 */
  let unlistenZip: UnlistenFn | null = null
  onMounted(() => {
    void watchLevelSetZipProgress((progress) => {
      zipProgress.value = progress
    }).then((unlisten) => {
      unlistenZip = unlisten
    })
  })
  onUnmounted(() => {
    unlistenZip?.()
    unlistenZip = null
  })

  /**
   * 详情面板模型：一个地图集当一个 MOD，随包插件缩进挂在它下面。
   *
   * 用 computed 而不是打开时算一份快照 —— 弹窗开着时切语言，标题、字段名与
   * 计数摘要要跟着变（`t()` 在 computed 里被执行，locale 变化会被追踪）。
   */
  const detail = computed<InstallDetailModel | null>(() => {
    const batch = pending.value
    if (!batch) return null
    return { ...levelSetImportDetail(batch.path, batch.sets, {
      previousVersions: set => batch.previousVersions[set.dir_name] ?? [],
    }), confirmLabel: t('modChanges.reviewSelection') }
  })

  /** 「立即安装」与「取消」的结论，交回 `importSource` 里那个 await。 */
  let settleApproval: ((approved: boolean) => void) | null = null

  function resolveApproval(approved: boolean) {
    const settle = settleApproval
    settleApproval = null
    settle?.(approved)
  }

  /** 打开详情弹窗，等用户点「立即安装」或放弃。 */
  function askApproval(path: string, sets: ZipLevelSet[]) {
    // 详情展示原版本，文件变更留给整链事务。
    const previousVersions = Object.fromEntries(sets.map(set => [set.dir_name, (set.conflicts ?? []).map(conflict => conflict.version)]))
    pending.value = { path, sets, previousVersions }
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
   * 暂存成功之后的统一链：详情选择 → 整链确认 → 事务导入。
   *
   * 本地文件/直链暂存与在线 Release 暂存只差「怎么拿到这份 ZIP」，拿到之后
   * 的确认、冲突处理、清理时机完全一样，所以只写这一份。
   */
  async function finishStaged(
    staged: StagedMapSource,
    successText?: () => string,
  ): Promise<boolean> {
    if (!staged.sets.length) {
      message.warning(t('levelSet.zipWithoutInfo'))
      return false
    }
    const sets = staged.sets
    if (!(await askApproval(staged.path, sets))) return false
    closePreview()
    // 运行中的游戏仍可安装纯地图，涉及 MOD 的方案由后端拒绝。
    const result = await changes.run({ roots: [], map_import: {
      zip_path: staged.path, selected: sets.map(set => set.dir_name), origin: staged.origin,
    } })
    zipProgress.value = null
    if (!result) return false
    message.success(successText?.() ?? t('levelSet.imported', { count: sets.length }))
    return true
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
    /*
     * 只有直链走下载管线（本地文件只读盘），所以只有直链需要「失败原地重试」。
     * 重试 = 重新走一遍这个函数：失败发生在下载这一步，后面的确认与导入
     * 都还没开始，不会重复执行已经成功过的步骤。
     */
    const token = source.kind === 'url'
      ? beginDownloadTask({
        label: t('levelSet.installMap'),
        retry: () => { void importSource(source, options) },
      })
      : null
    try {
      // 上一轮可能留下旧进度：暂存一开始先清空，避免显示上次的条目
      zipProgress.value = null
      const staged = await api.stageMapSource(source.kind, source.value)
      if (token !== null) endDownloadTask(token)
      // 识别结束，接下来是确认弹窗：不能把「正在读取」留在界面上
      zipProgress.value = null
      if (staged.staged) stagedFile = staged.path
      options?.onStaged?.()
      await finishStaged(staged)
    } catch (reason) {
      if (token !== null) failDownloadTask(token, reason)
      message.error(backendMessageOf(reason))
    } finally {
      // 不收弹窗：卸载失败时它要留着让用户处理后重试，成功与取消各自收过
      if (stagedFile) await api.discardMapSource(stagedFile).catch(() => undefined)
      zipProgress.value = null
      importing.value = false
    }
  }

  /**
   * 在线 Release 失败卡片上的标题：与下载进度的 label 同源（地图的本地化名）。
   * 地图源还没加载时退回通用文案，不让卡片空着。
   */
  function releaseLabel(mapId: string): string {
    const map = maps.value.find((entry) => entry.id === mapId)
    if (!map) return t('levelSet.installMap')
    return pickI18n(map.name_i18n, map.name) || t('levelSet.installMap')
  }

  /**
   * 安装一个在线地图 Release（地图下载页的「安装」）。
   *
   * 与本地导入共用同一条确认链：后端只把「下载资产 + 识别关卡集」做成暂存，
   * 剩下的与 ZIP 完全一样；暂存文件同样在 finally 里清掉（下载进 cache 的那份）。
   * Release 成功按地图名提示，本地导入只提示数量。
   */
  async function installRelease(
    mapId: string,
    tag?: string,
    assetName?: string,
    successText?: () => string,
  ): Promise<boolean> {
    if (importing.value) return false
    const map = maps.value.find((entry) => entry.id === mapId)
    const release = map && cachedInstallRelease(map.cached_releases, tag ?? map.latest_tag ?? undefined)
    if (!release) {
      message.error(t('mapSource.releaseMissing'))
      return false
    }
    const targetTag = release.tag_name
    importing.value = true
    let stagedFile: string | null = null
    /*
     * 重试 = 重新下载并重新走确认链：失败发生在下载这一步时，确认与导入
     * 都还没开始，不会把已经成功过的步骤再跑一遍。
     */
    const token = beginDownloadTask({
      label: releaseLabel(mapId),
      retry: () => { void installRelease(mapId, targetTag, assetName, successText) },
    })
    try {
      zipProgress.value = null
      const staged = await api.stageMapRelease(mapId, targetTag, assetName)
      endDownloadTask(token)
      // 下载 + 识别结束：确认弹窗才是接下来的主角
      zipProgress.value = null
      if (staged.staged) stagedFile = staged.path
      return await finishStaged(staged, successText)
    } catch (reason) {
      failDownloadTask(token, reason)
      message.error(backendMessageOf(reason))
      return false
    } finally {
      // 同上：弹窗开关只由 finishStaged 决定
      if (stagedFile) await api.discardMapSource(stagedFile).catch(() => undefined)
      zipProgress.value = null
      importing.value = false
    }
  }

  return {
    importing, importSource, installRelease, previewOpen, detail, confirmImport, discardImport,
    zipProgress, zipProgressText,
  }
}
