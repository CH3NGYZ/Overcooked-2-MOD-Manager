/**
 * 地图 ZIP 的「暂存 → 确认 → 导入」—— 唯一实现。
 *
 * 两个页面共用：地图下载页的「安装地图」来源卡片（本地选择 / 拖入窗口 / 直链），
 * 以及地图管理页的整窗拖放。写两份的代价不是多几十行，而是确认界面、覆盖提示、
 * 暂存清理与「装完重读」的时机各漂各的 —— 用户会在两页看到不同的东西。
 *
 * 确认界面不再是自拼的 `dialog.warning`：把每个地图集当成一个 MOD、随包插件
 * 与依赖表挂在它下面，交给和 MOD 安装共用的 `InstallDetailDialog` 展示。所以
 * 这里的职责只剩状态与顺序：暂存、打开详情、等用户表态、导入、清理暂存。
 *
 * 后端 `stage_map_source` 先把来源统一成一份可读 ZIP（本地文件原地引用、
 * 直链下载到 cache 并标记 `staged`），识别结果就是 `level_sets::
 * inspect_zip_with_progress` 的产物，所以「确认导入」只需要一份实现。
 *
 * 调用方只负责两件事：把来源交进来；导入成功后重读自己的关卡集列表。
 */
import { computed, onMounted, onUnmounted, ref, watch } from 'vue'
import type { UnlistenFn } from '@tauri-apps/api/event'
import { pickI18n, t } from '../i18n'
import { api, watchLevelSetZipProgress } from '../services/tauri'
import { useNotify } from '../useNotify'
import { levelSetImportDetail } from '../installDetail'
import { levelSetDependencyRows } from '../dependencyRows'
import { uninstallMapConflict } from '../installConflicts'
import { conflictCheckModel } from '../installCheck'
import { backendMessageOf } from '../locale/backend'
import { beginDownloadTask, endDownloadTask, failDownloadTask } from '../downloadTask'
import { repositoryIsAvailable } from '../repositoryAvailability'
import { useDashboard } from './useDashboardStore'
import { useInstallCheck } from './useInstallCheck'
import { useMapSources } from './useMapSources'
import type { InstallCheckRow } from '../installCheck'
import type { InstallDetailModel } from '../installDetail'
import type {
  InstallConflict, InstallSource, LevelSetZipProgress, StagedMapSource, ZipLevelSet,
} from '../types'

/** 正在等用户确认的那一批：ZIP 位置 + 识别出的关卡集。 */
interface PendingBatch {
  path: string
  sets: ZipLevelSet[]
}

export function useLevelSetImport(onImported?: () => void | Promise<void>) {
  const message = useNotify()
  /*
   * 依赖表的数据来源：Dashboard 的已装 MOD 与配置里的仓库、地图源的在线条目。
   * 确认界面里的集还没落盘，后端不会为它生成关系快照，所以关系只能在
   * 「身份匹配到在线地图 → 否则本地 ZIP 能带出的快照」这条链上现算。
   * 这两个都是模块级单例，取用不会额外触发请求。
   */
  const { config, mods } = useDashboard()
  const { maps } = useMapSources()
  const repositories = computed(() => {
    const current = config.value
    if (!current) return []
    return current.repositories.filter((repo) =>
      repositoryIsAvailable(repo, current.repository_sources, current.region, current.platform))
  })
  /** 正在处理一份来源：暂存 / 识别 / 等确认 / 导入，期间入口按钮转圈并禁用。 */
  const importing = ref(false)
  const pending = ref<PendingBatch | null>(null)
  /** 详情弹窗是否打开。 */
  const previewOpen = ref(false)

  /**
   * 同身份冲突的检查弹窗（共享的 `InstallCheckDialog`）。
   *
   * 与 `previewOpen` 一样，宿主视图只挂一个组件、绑这里的状态；「什么时候弹、
   * 等结论、弹窗被收掉算放弃」全在本 composable 里，两页因此必然一致。
   */
  const importCheck = useInstallCheck()

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
  const detail = computed<InstallDetailModel | null>(() =>
    pending.value
      ? levelSetImportDetail(pending.value.path, pending.value.sets, {
          /*
           * 弹窗里不做依赖跳转：跳走会把「等用户确认」的导入流程留在原地，
           * 回来还得重新识别一遍 ZIP。状态列只答「够不够」，不引导离开。
           */
          dependencies: (set) => levelSetDependencyRows(set, {
            maps: maps.value,
            mods: mods.value,
            repositories: repositories.value,
            translate: t,
            pickName: pickI18n,
            jumpToRepository: false,
          }),
        })
      : null)

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

  /** 导入成功后的收尾与文案：两条来源各自不同，确认/卸载/导入本身是同一份。 */
  interface FinishOptions {
    /**
     * 装完通知宿主。默认是构造时的 `onImported`；地图下载页的 Release 安装
     * 需要先同步依赖快照再重读关卡集（否则卡片状态会用旧快照算），所以它
     * 传自己的版本进来 —— 合并成两次扫描会白扫一遍全部地图。
     */
    afterImport?: () => void | Promise<void>
    /** 成功提示：Release 安装按地图名说「安装完成」，本地导入只说数量。 */
    successText?: (count: number) => string
    /**
     * 这次要装的地图名，只用于冲突检查的说明行（「安装「X」前…」）。
     *
     * 本地 ZIP 与直链没有可靠的目标名（暂存路径对用户没有意义），不传时
     * 说明行用通用那句「以下已安装内容与本次安装的身份相同…」。
     */
    targetLabel?: string
  }

  /**
   * 把各张卡的冲突表摊平成一份清单（按卸载句柄去重）。
   *
   * 去重必须在弹窗之前做：同一个已装目录可能在多张卡的冲突表里各出现一次
   * （同身份的多份副本），而卸载只能执行一次 —— 第二次一定报「找不到」。
   * 摊平后的下标就是检查行的 `index`，`execute` 据此取回要卸的那一项。
   */
  function collectConflicts(sets: readonly ZipLevelSet[]): InstallConflict[] {
    const seen = new Set<string>()
    const conflicts: InstallConflict[] = []
    for (const set of sets) {
      for (const conflict of set.conflicts ?? []) {
        if (seen.has(conflict.handle)) continue
        seen.add(conflict.handle)
        conflicts.push(conflict)
      }
    }
    return conflicts
  }

  /**
   * 冲突前置门控：**在问「要不要装」之前**把同身份旧装项处理掉。
   *
   * 为什么不能等用户点了「立即安装」再卸：那个顺序里用户已经确认过一次，
   * 卸载失败只能把详情弹窗留在原地，确认等于白点，还得再点一次。先处理冲突，
   * 处理完自动进确认，用户只做一次决定。
   *
   * 返回处理后的关卡集列表；`null` 表示用户取消或仍有失败，整次导入到此为止。
   * 成功时把冲突清空：那几项已经进了回收站，详情弹窗的冲突块不该再提示
   * （`InstallDetailDialog` 里那块保留作兜底渲染，只是不再有数据）。
   */
  async function resolveConflicts(
    sets: readonly ZipLevelSet[],
    targetLabel?: string,
  ): Promise<ZipLevelSet[] | null> {
    const conflicts = collectConflicts(sets)
    if (!conflicts.length) return [...sets]

    const ready = await importCheck.run(
      conflictCheckModel(conflicts, targetLabel ? { targetLabel } : {}),
      async (row: InstallCheckRow) => {
        const conflict = conflicts[row.index]
        // 理论上取得到：模型与数组同源同序。取不到说明清单变了，按「已不在原位」处理
        if (!conflict) throw new Error(t('levelSet.notFound'))
        await uninstallMapConflict(conflict)
      },
    )
    if (!ready) return null
    return sets.map((set) => ({ ...set, conflicts: [] }))
  }

  /**
   * 暂存成功之后的统一链：识别为空 → 处理冲突 → 确认 → 导入 → 重读。
   *
   * 本地文件/直链暂存与在线 Release 暂存只差「怎么拿到这份 ZIP」，拿到之后
   * 的确认、冲突处理、清理时机完全一样，所以只写这一份。
   *
   * 返回是否真的导入了：调用方据此决定要不要跑安装后的收尾（依赖快照等）。
   */
  async function finishStaged(
    staged: StagedMapSource,
    options: FinishOptions = {},
  ): Promise<boolean> {
    const afterImport = options.afterImport ?? onImported
    if (!staged.sets.length) {
      message.warning(t('levelSet.zipWithoutInfo'))
      return false
    }
    /*
     * 冲突先处理再问「要不要装」：同一个地图身份已经装过时，直接导入会让
     * 后端在回收旧目录与写入新目录之间打架，而卸载失败时详情弹窗只能留在
     * 原地 —— 用户等于白确认一次。取消/失败都中止整次导入（暂存由调用方
     * 的 finally 清掉）。
     */
    const sets = await resolveConflicts(staged.sets, options.targetLabel)
    if (!sets) return false
    if (!(await askApproval(staged.path, sets))) return false
    /*
     * 用户表态后先收起弹窗再导入：写盘期间进度由页面按钮转圈，弹窗留着
     * 会一直挂着「取消」按钮，看着像还能反悔。
     */
    closePreview()
    const selected = sets.map((set) => set.dir_name)
    const result = await api.importLevelSets(staged.path, selected)
    // 写盘结束：后面还有依赖快照与重读，进度不该停在最后一张上
    zipProgress.value = null
    message.success(options.successText?.(result.installed.length)
      ?? t('levelSet.imported', { count: result.installed.length }))
    await afterImport?.()
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
   * 返回是否装成功，调用方据此跑安装后的依赖快照同步。
   */
  async function installRelease(
    mapId: string,
    tag?: string,
    assetName?: string,
    options: FinishOptions = {},
  ): Promise<boolean> {
    if (importing.value) return false
    importing.value = true
    let stagedFile: string | null = null
    /*
     * 重试 = 重新下载并重新走确认链：失败发生在下载这一步时，确认与导入
     * 都还没开始，不会把已经成功过的步骤再跑一遍。
     */
    const token = beginDownloadTask({
      label: releaseLabel(mapId),
      retry: () => { void installRelease(mapId, tag, assetName, options) },
    })
    try {
      zipProgress.value = null
      const staged = await api.stageMapRelease(mapId, tag, assetName)
      endDownloadTask(token)
      // 下载 + 识别结束：确认弹窗才是接下来的主角
      zipProgress.value = null
      if (staged.staged) stagedFile = staged.path
      return await finishStaged(staged, { ...options, targetLabel: options.targetLabel ?? releaseLabel(mapId) })
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
    importCheck, zipProgress, zipProgressText,
  }
}
