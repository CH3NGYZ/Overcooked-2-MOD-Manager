import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
// 只读当前界面语言（用于让后端选择仓库的本地化名称）；不引入任何 UI 依赖
import { locale } from '../i18n'
import type {
  AppUpdateChannelEvent,
  AppUpdateCheckResult,
  AppUpdatePhaseEvent,
  BepInExInstallResult,
  Dashboard,
  DependencySyncResult,
  DropSourceKind,
  DiyLevelInfo,
  DownloadProgress,
  GameInfo,
  ModConfigDocument,
  ModPreview,
  ModRecycleEntry,
  ModSourceKind,
  OperationResult,
  PermissionStatus,
  Provider,
  PublicConfig,
  ReleaseInfo,
  RepositoryInstallOutcome,
  RepositoryReadme,
  RepositorySourceImportResult,
  RustLog,
  SettingsPatch,
  TokenPatch,
  UninstallLocalModResult,
  LevelSetFlattenReport,
  LevelSetInfo,
  LevelSetScanProgress,
  LevelSetZipProgress,
  MapInfo,
  StagedMapSource,
} from '../types'

let gameStateRevision = 0
let latestGameState: GameInfo | undefined

async function readDashboard(): Promise<Dashboard> {
  const revision = gameStateRevision
  const dashboard = await invoke<Dashboard>('get_dashboard')
  // MOD 扫描可能慢于游戏状态事件：保留较新的事件快照，避免旧状态覆盖。
  if (revision !== gameStateRevision && latestGameState) dashboard.game = latestGameState
  return dashboard
}

/**
 * v2 前端调用面：只封装新版 UI 实际使用的命令。
 * 教程、社区搜索、代理管理、导入导出、配置编辑器等 v1 功能已裁剪；
 * 后端命令仍全部注册，供后续版本按需恢复。
 */
export const api = {
  dashboard: readDashboard,
  config: () => invoke<PublicConfig>('get_config'),
  readRustLog: () => invoke<RustLog>('read_rust_log'),
  readFrontendLog: () => invoke<RustLog>('read_frontend_log'),
  readCrashLog: () => invoke<RustLog>('read_crash_log'),
  /** 清空一份日志并拿回清空后的内容（`kind` 是 rust/frontend/crash，后端白名单校验）。 */
  clearLog: (kind: string) => invoke<RustLog>('clear_log', { kind }),
  writeFrontendLog: (level: string, message: string) => invoke<void>('write_frontend_log', { level, message }),
  saveModNote: (dllFile: string, note: string) => invoke<OperationResult>('save_mod_note', { dllFile, note }),
  /** MOD 在 BepInEx/config 下的配置文件（含 revision 防覆盖）。 */
  readModConfig: (dllFile: string) => invoke<ModConfigDocument>('read_mod_config', { dllFile }),
  saveModConfig: (dllFile: string, revision: string, content: string) =>
    invoke<ModConfigDocument>('save_mod_config', { dllFile, revision, content }),
  /** BepInEx 框架自身的配置文件（`BepInEx/config/BepInEx.cfg`）。 */
  readBepInExConfig: () => invoke<ModConfigDocument>('read_bepinex_config'),
  saveBepInExConfig: (revision: string, content: string) =>
    invoke<ModConfigDocument>('save_bepinex_config', { revision, content }),
  uninstallLocalMod: (dllFile: string, allowUnmanaged = false) => invoke<UninstallLocalModResult>('uninstall_local_mod', { dllFile, allowUnmanaged }),
  uninstallModDll: (dllFile: string, packageId: string) => invoke<OperationResult>('uninstall_mod_dll', { dllFile, packageId }),
  getModRecycleBin: () => invoke<ModRecycleEntry[]>('get_mod_recycle_bin'),
  restoreRecycledMod: (id: string) => invoke<OperationResult>('restore_recycled_mod', { id }),
  deleteRecycledMod: (id: string) => invoke<OperationResult>('delete_recycled_mod', { id }),
  /** 清空回收站：彻底删除全部记录。 */
  deleteAllRecycledMods: () => invoke<OperationResult>('delete_all_recycled_mods'),
  detectGame: () => invoke<GameInfo>('detect_game'),
  setGamePath: (path: string) => invoke<GameInfo>('set_game_path', { path }),
  resetGamePath: () => invoke<GameInfo>('reset_game_path'),
  saveSettings: (patch: SettingsPatch) => invoke<PublicConfig>('save_settings', { patch }),
  /** 保存 / 清除 API 个人 Token（非空字符串覆盖，空字符串保持不变）。 */
  saveTokens: (patch: TokenPatch) => invoke<PublicConfig>('save_tokens', { patch }),
  /**
   * 刷新单个仓库的 Release 列表。
   *
   * 后端对 Release 列表有 5 分钟缓存：批量刷新、自动刷新、切页回来都直接
   * 读缓存，不再逐仓打 API。只有**用户主动点刷新**时才传 `force: true`，
   * 因为那个动作的语义就是「现在就要最新的」。
   */
  refreshRepository: (repositoryId: string, force = false) =>
    invoke<ReleaseInfo[]>('refresh_repository', { repositoryId, force }),
  /**
   * 把线上依赖声明同步进本地清单（MOD 安装清单 + 地图依赖清单）。
   *
   * 依赖只写在仓库源文档里，源下线或文档改版后本地就查不到了；所以安装时
   * 固化一份，每次刷新完再按线上声明重写一遍 —— 在线与本地不一致时以在线
   * 为准。返回实际改写的条数（0 = 本来就一致）。
   */
  syncDependencySnapshots: () => invoke<DependencySyncResult>('sync_dependency_snapshots'),
  /** 从源文档地址（raw / 远程 TOML）重新拉取并应用该仓库源。 */
  importRepositorySource: (sourceId: string, sourceUrl: string, builtinFallback = false) =>
    invoke<RepositorySourceImportResult>('import_repository_source', { sourceId, sourceUrl, builtinFallback }),
  /**
   * 刷新前清空这些源（含后代）下的仓库与子源缓存，返回清理后的配置。
   *
   * 为什么要先清：不清的话，前端会继续显示上一次的仓库列表，
   * 直到新文档回来才替换 —— 用户看到「旧列表先出现、再跳变」。
   * 清空后列表立即变空（配合页面的加载态），权威数据只来自本次 raw 拉取。
   */
  clearRepositorySourceCache: (sourceIds: string[]) =>
    invoke<PublicConfig>('clear_repository_source_cache', { sourceIds }),
  releases: (repositoryId: string) => invoke<ReleaseInfo[]>('get_repository_releases', { repositoryId }),
  /**
   * 批量读取仓库 owner 头像，返回 `{provider}:{owner} -> data URL`。
   *
   * 后端按 owner 去重、缓存到磁盘、按 Raw 代理候选下载；取不到的 owner
   * **不在结果里**（不是空串），前端据此回落到平台图标。
   */
  repositoryAvatars: (owners: { provider: Provider; owner: string }[]) =>
    invoke<Record<string, string>>('get_repository_avatars', { owners }),
  installBepInEx: (reinstall: boolean, provider: Provider) => invoke<BepInExInstallResult>('install_bepinex', { reinstall, provider }),
  repairNativeBepInExOverlay: () => invoke<OperationResult>('repair_native_bepinex_overlay'),
  uninstallBepInEx: () => invoke<OperationResult>('uninstall_bepinex'),
  backupBepInEx: () => invoke<OperationResult>('backup_bepinex'),
  restoreBepInEx: (backupPath: string) => invoke<OperationResult>('restore_bepinex', { backupPath }),
  deleteBepInExBackup: (backupPath: string) => invoke<OperationResult>('delete_bepinex_backup', { backupPath }),
  /** 仓库 README（详情面板按需加载；requestId 用于取消）。 */
  repositoryReadme: (repositoryId: string, requestId: string) =>
    invoke<RepositoryReadme>('get_repository_readme', { repositoryId, requestId }),
  cancelRepositoryReadme: (requestId: string) => invoke<void>('cancel_repository_readme', { requestId }),
  /** 用系统默认浏览器打开外部链接（Release 页 / 源码仓库）。 */
  openReleasePage: (url: string) => invoke<OperationResult>('open_release_page', { url }),
  /**
   * 安装仓库资产。
   *
   * `previewOnly` 为真时只下载并解析、不写入游戏目录，返回结果里带
   * `preview` —— 仓库卡片的「安装 / 重装 / 更新」走这条路径先进预览弹窗，
   * 用户确认后再调 `confirmModInstall`。
   *
   * `assetName` 是安装页下拉里选中的资产；不传时后端按 `asset_rules` 自动选。
   * 必须与界面上显示的文件一致，否则会出现「选的 A、下载的 B」。
   */
  installRepository: (repositoryId: string, tag?: string, previewOnly = false, assetName?: string) =>
    invoke<RepositoryInstallOutcome>('install_repository', {
      repositoryId, tag, previewOnly, assetName,
      // 后端按它选仓库的本地化名称（下载进度标签 + 安装预览标题）
      locale: locale.value,
    }),
  launchNormal: () => invoke<OperationResult>('launch_normal'),
  /** 停止厨房：Steam 没有停止协议，由管理器结束游戏进程。 */
  stopGame: () => invoke<OperationResult>('stop_game'),
  /**
   * 立刻重算游戏状态并返回最新快照。
   *
   * 观察器每秒推进一次、只在变化时推送；用户切回窗口、点完停止、或某次
   * 操作因 `game.running` 被拒时，界面上的值可能还停在上一轮。主动拉一次
   * 让按钮与各页的可用状态当场收敛，不必等下一轮观察。
   */
  refreshGameState: () => invoke<GameInfo>('refresh_game_state'),
  launchNoMod: () => invoke<OperationResult>('launch_no_mod'),
  switchWinhttp: (enabled: boolean) => invoke<OperationResult>('switch_winhttp', { enabled }),
  toggleLocalMod: (dllFile: string, enabled: boolean) => invoke<OperationResult>('toggle_local_mod', { dllFile, enabled }),
  toggleLocalModPackage: (packageId: string, enabled: boolean) => invoke<OperationResult>('toggle_local_mod_package', { packageId, enabled }),
  /** 一次性启用/禁用 `BepInEx/plugins` 下全部插件 DLL（失败整体回滚）。 */
  setAllLocalModsEnabled: (enabled: boolean) =>
    invoke<OperationResult>('set_all_local_mods_enabled', { enabled }),
  openDevtools: () => invoke<OperationResult>('open_devtools'),
  resetUi: () => invoke<OperationResult>('reset_ui'),
  /** 把生成好的多彩名字推送给正在运行的多彩名字 MOD（后端逐个端口尝试）。 */
  pushColorfulName: (name: string) => invoke<OperationResult>('push_colorful_name', { name }),
  /** 复制文本到系统剪贴板。 */
  copyText: (text: string) => invoke<OperationResult>('copy_text', { text }),
  openGameDirectory: () => invoke<OperationResult>('open_game_directory'),
  openModDirectory: () => invoke<OperationResult>('open_mod_directory'),
  /** 打开 `<game>/BepInEx`（框架自身文件所在处，不是 plugins）。 */
  openBepInExDirectory: () => invoke<OperationResult>('open_bepinex_directory'),
  /** DIYLevel 自定义关卡框架状态（GUID 判定，与 MOD 列表同源）。 */
  diyLevelInfo: () => invoke<DiyLevelInfo>('get_diy_level_info'),

  /*
   * 地图（DIYLevel 关卡集）管理。
   *
   * 与 MOD 的 api 刻意分开：两者扫描源、身份和安装单元都不同。共用的是
   * 界面组件与样式，不是数据通道。
   */
  /** 列出全部关卡集（启用与停用）。只读，游戏运行时也可调用。 */
  levelSets: () => invoke<LevelSetInfo[]>('get_level_sets'),
  /**
   * 整理地图目录：把 `info` 埋在子文件夹里的目录搬成规范的 `levels/<集名>/`。
   *
   * 扫描口径只读 `levels` 的第一层（与游戏插件一致），所以嵌套的集在游戏里
   * 本来就读不到。`dryRun` 只探测候选（不写盘、不要求关游戏），界面先用它
   * 判断要不要提示用户；正式整理会搬文件并把清空的空壳移进回收站。
   */
  flattenLevelSets: (dryRun: boolean) =>
    invoke<LevelSetFlattenReport>('flatten_level_sets', { dryRun }),
  /**
   * 启用或停用一个关卡集（移动目录）。
   *
   * 游戏运行时会失败并返回 `levelSet.gameRunning` —— 移动 `levels/` 里的
   * 目录可能与 DLL 正在做的枚举冲突，这条约束在实机验证前不放行。
   */
  setLevelSetEnabled: (dirName: string, enabled: boolean) =>
    invoke<OperationResult>('set_level_set_enabled', { dirName, enabled }),
  /**
   * 批量切换**全部**关卡集（地图管理页的「全部启用 / 全部禁用」）。
   *
   * 后端在一次操作锁里扫描磁盘真实状态、只切换需要变的目录；中途失败会
   * 按逆序回滚，不会留下半开半关的列表。游戏运行时会整体拒绝。
   */
  setAllLevelSetsEnabled: (enabled: boolean) =>
    invoke<OperationResult>('set_all_level_sets_enabled', { enabled }),
  /**
   * 判断一个本地文件是地图包、MOD 包还是都不是。
   *
   * 拖放先过这里再决定走哪条链路：地图包即使带着修复插件也算地图包，
   * 否则「拖地图到 MOD 页」会被装成一个 0 DLL 的空 MOD。
   */
  classifyDropSource: (path: string) => invoke<DropSourceKind>('classify_drop_source', { path }),
  /**
   * 从本地 ZIP 导入选定的关卡集。
   *
   * 已存在的同名关卡集会被回收（进回收站），不是直接删除。游戏运行时允许。
   * 期间后端逐份发 `level-set-zip-progress`（`info` 阶段再识别一遍、`install`
   * 阶段写盘），进度显示在来源卡片与整页遮罩上。
   */
  importLevelSets: (zipPath: string, selected: string[]) =>
    invoke<{ installed: string[] }>('import_level_sets', { zipPath, selected }),
  /**
   * 暂存一份地图来源（本地 ZIP 或在线直链），返回里面识别出的关卡集。
   *
   * 「安装地图」的本地文件、拖入、直链三个入口都先走这里：来源统一落到后端
   * 可读的 ZIP 上，`inspect` 与「确认导入」因此只有一份实现。直链会下载到
   * cache 并标记 `staged`，导入结束后由 `discardMapSource` 清理。
   */
  stageMapSource: (kind: 'local' | 'url', value: string) =>
    invoke<StagedMapSource>('stage_map_source', { source: { kind, value } }),
  /**
   * 丢弃 `stageMapSource` 暂存的 ZIP（只删 cache 内的文件）。
   *
   * 导入成功 / 失败 / 用户取消都要调用，否则每下载一次地图就在 cache 里留一份。
   * 不负责任务失败：清理失败不影响导入结果，缓存文件下次启动仍会被清理。
   */
  discardMapSource: (path: string) => invoke<void>('discard_map_source', { path }),
  /**
   * 在资源管理器中打开一个关卡集目录。
   *
   * 只读操作：游戏运行时也允许。`enabled` 决定去 `levels/` 还是 `disabled/`
   * 找 —— 目录名相同但位置不同的两个集不会互相指错。
   */
  openLevelSetDirectory: (dirName: string, enabled: boolean) =>
    invoke<OperationResult>('open_level_set_directory', { dirName, enabled }),
  /**
   * 删除一个关卡集。
   *
   * 与 MOD 删除同一套策略：默认移入回收站（跟随 `recycle_deleted_mods`），
   * 关闭回收时永久删除。移动目录在游戏运行时会被后端拒绝（`levelSet.gameRunning`）。
   */
  deleteLevelSet: (dirName: string, enabled: boolean) =>
    invoke<OperationResult>('delete_level_set', { dirName, enabled }),
  /**
   * 清空地图（关卡集）`info` 的解析缓存，返回删除的磁盘缓存文件数。
   *
   * 进程内与磁盘两级一起清 —— 只清一侧用户会觉得「点了没反应」：留内存的
   * 命中就直接返回旧结果，留磁盘的下一次解析立刻读回同一份。
   *
   * 缓存的键是文件**内容哈希**，文件不改就永远命中。所以解析结果被旧版本
   * 写错过、或想确认某份 `info` 到底能解析成什么，都得先清一次。清完不影响
   * 已安装的地图，下次进入地图页 / 总览会重新解析并写回缓存。
   */
  clearLevelSetCache: () => invoke<number>('clear_level_set_cache'),

  /*
   * 地图下载：与 MOD 仓库同一套刷新与安装语义，只是数据模型不同。
   */
  /** 列出当前地区可用的地图（只读，不联网）。 */
  maps: () => invoke<MapInfo[]>('get_maps'),
  /**
   * 刷新整棵地图源树（索引 → 子源 → 地图），返回刷新后的地图列表。
   *
   * 与 MOD 仓库源的区别：地图源树不持久化成一棵可编辑的源树，只是归类
   * 方式，所以没有「刷新单个源」这个粒度。
   *
   * `passId` 是「同一次用户点击」的批次号，与随后的 `refreshMapReleases`
   * 共用：tag 模式下同一次加载要为同一个仓库的多个版本取资产，后端按它去重。
   */
  refreshMapSources: (passId?: string) =>
    invoke<MapInfo[]>('refresh_map_sources', { passId }),
  /**
   * 刷新一个地图的 Release 列表（5 分钟缓存）。
   *
   * `force` 由「用户主动点刷新」传入：那个动作的语义就是「现在就要最新的」。
   * `passId` 与 `refreshMapSources` 是同一个批次号：tag 模式下同一批会为同一
   * 仓库的多个版本各要一次资产列表，后端按它把同批请求合并成一次。
   */
  refreshMapReleases: (mapId: string, force = false, passId?: string) =>
    invoke<ReleaseInfo[]>('refresh_map_releases', { mapId, force, passId }),
  /**
   * 暂存一次地图 Release 安装（下载资产并识别其中的关卡集）。
   *
   * 与本地文件/直链共用同一条「先暂存、再确认导入」的路：返回的 `sets`
   * 带冲突报告，确认界面复用 `InstallDetailDialog`；`staged` 为真时导入结束
   * 必须 `discardMapSource` 清理。
   * `assetName` 是详情页下拉里选中的资产；不传时后端按地图源的 `asset_rules`
   * 自动选，与 MOD 安装完全同一条规则。
   * `locale` 用于下载进度标签：进度事件在下载过程中持续发出，
   * 名称必须在开始前按界面语言算好。
   */
  stageMapRelease: (mapId: string, tag?: string, assetName?: string) =>
    invoke<StagedMapSource>('stage_map_release', {
      mapId, tag, assetName, locale: locale.value,
    }),
  /** 切换 DIYLevel 启用状态（复用 MOD 的 .dll/.dll.disabled 逻辑）。 */
  setDiyLevelEnabled: (enabled: boolean) =>
    invoke<OperationResult>('set_diy_level_enabled', { enabled }),
  /** 打开 DIYLevel 的地图目录（不自动创建）。 */
  openDiyLevelDirectory: () => invoke<OperationResult>('open_diy_level_directory'),
  /** 备份 DIYLevel 目录（只含 `BepInEx/plugins/OC2DIYLevel`）。 */
  backupDiyLevel: () => invoke<OperationResult>('backup_diy_level'),
  /** 恢复 DIYLevel 备份（先删现有目录再整体还原）。 */
  restoreDiyLevel: (backupPath: string) => invoke<OperationResult>('restore_diy_level', { backupPath }),
  deleteDiyLevelBackup: (backupPath: string) =>
    invoke<OperationResult>('delete_diy_level_backup', { backupPath }),
  /** 卸载 DIYLevel 框架：自动备份后按安装清单回收。 */
  uninstallDiyLevel: () => invoke<OperationResult>('uninstall_diy_level'),
  /** 清除头像磁盘缓存，返回删除的文件数。 */
  clearRepositoryAvatars: () => invoke<number>('clear_repository_avatars'),
  openModFileLocation: (dllFile: string) => invoke<OperationResult>('open_mod_file_location', { dllFile }),
  openModLogsDirectory: (dllFile: string) => invoke<OperationResult>('open_mod_logs_directory', { dllFile }),
  openBackupDirectory: (backupPath?: string) => invoke<OperationResult>('open_backup_directory', { backupPath: backupPath ?? null }),
  openUpdateRepository: () => invoke<OperationResult>('open_update_repository'),
  exitApp: () => invoke<void>('exit_app'),
  /** 当前进程权限与游戏/数据目录可写性自检。 */
  permissionStatus: () => invoke<PermissionStatus>('permission_status'),
  /** 以管理员身份重启（仅文件访问被拒时由用户主动触发）。 */
  relaunchAsAdmin: () => invoke<OperationResult>('relaunch_as_admin'),
  cancelDownload: () => invoke<OperationResult>('cancel_download'),
  /**
   * 安装程序在安装时写入的界面语言（`install-language.txt`）。
   *
   * 仅用于「首次启动且用户还没手动选过语言」的场景；后端只返回受支持的
   * 五种语言，其它内容一律为 null，调用方不必再校验。
   */
  installLanguage: () => invoke<string | null>('get_install_language'),
  /**
   * 搜索一个平台的社区仓库并合并进配置，返回合并后的完整配置。
   *
   * 搜索结果由后端 `merge_results` 落盘：与官方源同坐标的社区副本会被标成
   * `enabled=false` 整条隐藏，所以前端拿到配置后直接写回 Dashboard 即可。
   * 同一平台并发搜索时后端返回「社区仓库正在搜索」。
   */
  refreshCommunityRepositories: (provider: Provider) =>
    invoke<PublicConfig>('refresh_community_repositories', { provider }),
  checkAppUpdate: (locale: 'zh-CN' | 'zh-TW' | 'en' | 'ko' | 'ja') => invoke<AppUpdateCheckResult>('check_app_update', { locale }),
  installAppUpdate: () => invoke<OperationResult>('install_app_update'),
  chooseGameDirectory: () => open({ multiple: false, directory: true }),
  /** 选择要导入的地图 ZIP。 */
  chooseLevelSetZip: () => open({
    multiple: false,
    directory: false,
    filters: [{ name: 'ZIP', extensions: ['zip'] }],
  }),
  /** 选择要安装的本地 MOD 文件（DLL 或 ZIP）。 */
  chooseModFile: () => open({
    multiple: false,
    directory: false,
    filters: [{ name: 'MOD', extensions: ['dll', 'zip'] }],
  }),
  /** 安装本地 MOD 文件（DLL 或 ZIP）。 */
  installLocalMod: (sourcePath: string) => invoke<OperationResult>('install_local_mod', { sourcePath }),
  /** 从 zip / dll 直链下载并安装（后端已有直连下载 + 布局判定 + 清单）。 */
  installOnlineMod: (url: string) => invoke<OperationResult>('install_online_mod', { url }),
  /** 解析安装源（本地文件 / 在线直链），返回预览而不安装。 */
  analyzeModSource: (kind: ModSourceKind, value: string) =>
    invoke<ModPreview>('analyze_mod_source', { source: { kind, value } }),
  /** 用户在预览里确认后真正安装。 */
  confirmModInstall: (previewId: string) => invoke<OperationResult>('confirm_mod_install', { previewId }),
  /** 取消预览，删除暂存。 */
  cancelModPreview: (previewId: string) => invoke<void>('cancel_mod_preview', { previewId }),
}

export function watchDownloadProgress(callback: (progress: DownloadProgress) => void): Promise<UnlistenFn> {
  return listen<DownloadProgress>('download-progress', (event) => callback(event.payload))
}

export function watchGameState(callback: (game: GameInfo) => void): Promise<UnlistenFn> {
  return listen<GameInfo>('game-state-changed', (event) => {
    latestGameState = event.payload
    gameStateRevision += 1
    callback(event.payload)
  })
}

/**
 * 关卡集扫描进度（`get_level_sets` 执行期间逐份上报）。
 *
 * 首次进地图页要解析几十到上百 MB 的 `info`，转圈条下面靠它显示
 * 「已读取 x/y」与「正在读取的文件夹」，否则用户分不出是在跑还是卡死。
 */
export function watchLevelSetScan(callback: (progress: LevelSetScanProgress) => void): Promise<UnlistenFn> {
  return listen<LevelSetScanProgress>('level-set-scan-progress', (event) => callback(event.payload))
}

/**
 * 来源 ZIP 的识别 / 导入进度（`stage_map_source`、`stage_map_release` 与
 * `import_level_sets` 执行期间逐份上报）。
 *
 * 与 `watchLevelSetScan` 的分工：那个读游戏目录里已落盘的关卡集，这个读
 * 用户刚拖进来 / 刚下载的 ZIP —— 两种等待都要在转圈下面给出真实进度。
 */
export function watchLevelSetZipProgress(callback: (progress: LevelSetZipProgress) => void): Promise<UnlistenFn> {
  return listen<LevelSetZipProgress>('level-set-zip-progress', (event) => callback(event.payload))
}

/**
 * 「磁盘上的关卡集变了」。
 *
 * 后端 `level_watch` 递归盯着 `levels` 与停用目录，按 600ms 防抖压成一声；
 * 用户在资源管理器里删掉一个集、或别的工具往这个目录写东西时，页面靠它
 * 自动重读，不必让用户回去点刷新。载荷为空 —— 事件只表达「该重读了」，
 * 差异由重读本身给出。
 */
export function watchLevelSetsChanged(callback: () => void): Promise<UnlistenFn> {
  return listen('level-sets-changed', () => callback())
}

export function watchAppUpdatePhase(callback: (progress: AppUpdatePhaseEvent) => void): Promise<UnlistenFn> {
  return listen<AppUpdatePhaseEvent>('app-update-phase', (event) => callback(event.payload))
}

export function watchAppUpdateChannel(callback: (progress: AppUpdateChannelEvent) => void): Promise<UnlistenFn> {
  return listen<AppUpdateChannelEvent>('app-update-channel', (event) => callback(event.payload))
}
