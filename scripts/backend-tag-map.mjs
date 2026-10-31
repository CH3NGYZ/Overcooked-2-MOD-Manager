/**
 * 后端中文串 → tag 的迁移清单。
 *
 * ## 为什么需要这份清单
 *
 * 后端有 445 条用户可见中文串，散落在 21 个模块里。逐条手改的问题：
 *   1. 同一条文案在多处出现（「操作锁异常」出现十几次），手改必然漏；
 *   2. 改完要在语言表里再加一遍五语，漏一个人工无法发现；
 *   3. 无法核对「还有哪些没迁」。
 *
 * 所以先建这份**唯一真相源**：`原文 → tag`。随后：
 *   · `apply-backend-tags.mjs` 按清单批量替换 Rust 源码；
 *   · `verify` 模式反向扫描源码，报告还有哪些中文串没进清单。
 *
 * 命名规则：`<域>.<对象>.<动作/原因>`，全小写、点分层。
 * 域固定为下面 DOMAINS 里那几个，与前端 `src/locale/types.ts` 对应。
 */

/** 后端 tag 的域。与前端语言表的域一一对应。 */
export const TAG_DOMAINS = [
  'lock',        // 操作锁
  'game',        // 游戏目录与运行状态
  'framework',   // BepInEx / DIYLevel 框架
  'mods',        // 本地 MOD 安装、开关、卸载
  'backup',      // 框架备份
  'recycle',     // 回收站
  'download',    // 下载与归档
  'archive',     // ZIP 解压与安装布局
  'update',      // 程序自更新
  'source',      // 仓库源与代理源
  'config',      // 配置读写与校验
  'proxy',       // 代理清单解析
  'community',   // 社区仓库发现
  'avatar',      // 头像
  'readme',      // 仓库简介
  'diyLevel',    // DIYLevel 设置
  'wine',        // Wine / CrossOver
  'migrate',     // 更新迁移
]

/**
 * 迁移清单：**中文原文 → tag**。
 *
 * 只列用户可能看到的（`Err(...)`、`ok_or(...)`、`message:`、`ConfigError`）；
 * 纯日志不进这里 —— 日志是给开发者看的，保持中文更利于排查，
 * 且改成 tag 反而会降低 `rust.log` 的可读性与检索性。
 */
export const ENTRIES = {
  // ===== 操作锁 =====
  '操作锁异常': 'lock.poisoned',

  // ===== 游戏目录与运行 =====
  '尚未找到厨房目录': 'game.notFound',
  '厨房目录已变化，请重新添加模组': 'game.directoryChanged',
  '厨房设置已变化，请重试': 'game.settingsChanged',
  '当前模式未找到厨房可执行文件': 'game.executableMissing',
  'Windows 仅支持 Windows 版厨房': 'game.windowsOnly',
  '当前平台不支持启动厨房': 'game.unsupportedPlatform',
  '《胡闹厨房2》正在运行，请先关闭后重试': 'game.running',
  '已请求 Steam 启动厨房，请在 Steam 中确认启动': 'game.steamLaunchRequested',
  '所选目录中没有 Overcooked2.exe': 'game.exeNotFound',
  '所选目录中没有 Overcooked2.app': 'game.appNotFound',

  // ===== MOD 安装 =====
  '所选文件不存在': 'mods.fileMissing',
  '文件名无效': 'mods.invalidFileName',
  '只支持 DLL 或 ZIP 文件': 'mods.unsupportedFileType',
  '找不到对应的已安装 MOD': 'mods.installedNotFound',
  '找不到对应的 MOD DLL 文件': 'mods.dllNotFound',
  '当前 DLL 不属于这个 MOD 包': 'mods.dllNotInPackage',
  '没有安装清单，拒绝删除未知文件': 'mods.unknownFiles',
  '该 MOD 不是通过本管理器安装的，管理器中只会删除这个 DLL，其他相关文件需手动处理。是否继续？': 'mods.unmanagedConfirmation',
  '当前 MOD 与其他 MOD 一并安装': 'mods.packageConfirmation',
  'MOD 已启用': 'mods.enabled',
  'MOD 已禁用': 'mods.disabled',
  'MOD 包已启用': 'mods.packageEnabled',
  'MOD 包已禁用': 'mods.packageDisabled',
  '只能切换 BepInEx/plugins 下的 DLL 文件': 'mods.notInPlugins',
  '找不到已禁用的 MOD DLL 文件': 'mods.disabledDllMissing',
  '找不到已启用的 MOD DLL 文件': 'mods.enabledDllMissing',
  '目标 MOD 文件已存在，无法切换': 'mods.toggleTargetExists',
  '没有安装清单，无法切换 MOD 包': 'mods.packageManifestMissing',
  '这个 MOD 包没有可切换的插件 DLL': 'mods.packageNoToggleableDll',
  '没有找到任何 MOD 插件 DLL': 'mods.noPluginDll',
  '不能操作游戏目录之外的文件': 'mods.outsideGameDirectory',
  'MOD 备注已保存': 'mods.noteSaved',
  'MOD 备注已清除': 'mods.noteCleared',

  // ===== 备份与恢复 =====
  '备份目录不存在': 'backup.notFound',
  '备份目录不可用': 'backup.unavailable',
  '只能操作该框架的时间戳备份': 'backup.wrongKind',
  '备份目录不在允许的备份位置': 'backup.outsideRoot',
  '备份与框架目录重叠，已停止操作以避免递归复制。请把管理器安装到游戏目录外，或把数据目录移出 BepInEx。': 'backup.overlap',
  '备份目标与源目录是同一个目录，已停止操作以避免覆盖数据。': 'backup.sameDirectory',
  '未找到可回收的框架文件。': 'backup.noReclaimableFilesShort',

  // ===== 框架 =====
  'BepInEx 未安装': 'framework.bepinexMissing',
  'BepInEx 已安装，如需覆盖请使用重新安装': 'framework.alreadyInstalled',
  'BepInEx 安装完成': 'framework.bepInExInstalled',
  'BepInEx 框架未安装（仅存在 MOD 目录），无需卸载': 'framework.bepinexOnlyResidue',
  'BepInEx 安装失败，备份恢复未完成，请从框架备份恢复': 'framework.installFailedRollbackIncomplete',
  '尚未安装 DIYLevel 框架': 'framework.diyLevelMissing',
  'DIYLevel 框架已启用': 'framework.diyLevelEnabled',
  'DIYLevel 框架已禁用': 'framework.diyLevelDisabled',
  '已打开 DIYLevel 目录': 'framework.diyLevelDirOpened',
  '已打开 BepInEx 目录': 'framework.bepInExDirOpened',
  'BepInEx 目录不存在，请重新检测框架状态': 'framework.bepInExDirMissing',
  '尚未安装 DIYLevel 框架，或安装结构与预期不同': 'framework.diyLevelStructureUnexpected',
  'Steam Overlay 注入修复完成': 'framework.overlayRepaired',
  '未找到 DIYLevel 目录': 'framework.diyLevelDirMissing',
  '未找到 BepInEx 安装清单': 'framework.bepInExManifestMissing',

  // ===== 下载与归档 =====
  '下载已取消': 'download.cancelled',
  '下载文件为空': 'download.empty',
  '下载长度不完整': 'download.incomplete',
  'SHA-256 校验失败': 'download.checksumFailed',
  'ZIP 文件名无效': 'archive.invalidZipName',
  '路径逃逸游戏目录': 'archive.pathEscape',
  '所选文件没有可安装内容': 'archive.noInstallableContent',
  '所选文件已经位于安装目标，不能覆盖自身': 'archive.sourceEqualsTarget',
  '下载完成但没有解析出安装内容': 'archive.noParsedContent',

  // ===== 程序更新 =====
  '更新清单格式无效': 'update.manifestInvalid',
  '更新清单不是稳定版本': 'update.manifestNotStable',
  '更新中没有匹配的安装包': 'update.noMatchingPackage',
  '更新清单或签名不可用，已停止安装': 'update.manifestOrSignatureUnavailable',
  '更新清单与所选平台安装包不一致，已停止安装': 'update.manifestPlatformMismatch',
  '更新清单请求失败': 'update.manifestRequestFailed',
  '更新清单过大': 'update.manifestTooLarge',
  '更新清单读取失败': 'update.manifestReadFailed',
  '更新清单地址无效': 'update.manifestUrlInvalid',
  '当前程序版本无效': 'update.currentVersionInvalid',
  '无法读取当前运行目录': 'update.exeDirectoryUnavailable',
  '当前平台或架构不支持自动更新，请手动更新': 'update.platformUnsupported',
  '当前安装格式不支持自动更新，请手动更新': 'update.formatUnsupported',
  '安装器更新渠道不可用': 'update.channelUnavailable',
  '已请求打开更新页面': 'update.pageOpened',
  '打开更新页面失败': 'update.pageOpenFailed',

  // ===== 仓库源与代理 =====
  '仓库源地址不能为空': 'source.addressEmpty',
  '仓库源地址不能包含认证信息': 'source.addressHasCredentials',
  '内置仓库源路径不安全': 'source.builtinPathUnsafe',
  '内置仓库源必须是 TOML 文件': 'source.builtinNotToml',
  '仓库源不是内置文件': 'source.notBuiltin',
  '仓库源 ID 和昵称不能为空': 'source.idOrNicknameEmpty',
  '代理来源地址必须是 HTTPS URL': 'source.proxyNotHttps',
  '仓库源文件超过 1 MiB': 'source.tooLarge',
  '仓库源至少需要包含一个仓库或一个子仓库源': 'source.emptyDocument',
  '仓库源文档超过大小限制': 'source.documentTooLarge',
  '代理来源超过大小限制': 'proxy.sourceTooLarge',
  '文件不是有效 UTF-8 文本': 'proxy.invalidUtf8',
  '代理脚本缺少 download_url_us 列表': 'proxy.missingDownloadList',
  '文件格式、字段或代理条目校验失败': 'proxy.validationFailed',
  '仓库不存在': 'source.repositoryMissing',
  '仓库源不存在': 'source.sourceMissing',
  '仓库 ID 不能为空': 'source.idEmpty',

  // ===== 配置 =====
  '配置读取失败': 'config.readFailed',
  '配置保存失败': 'config.saveFailed',
  '配置文件格式无效': 'config.invalidFormat',
  '配置目录不可用': 'config.directoryUnavailable',
  '适用地区无效，请选择中国大陆或非中国大陆': 'config.invalidRegion',
  '安装目录不安全': 'config.installTargetUnsafe',
  '安装目录不能是绝对路径': 'config.installTargetAbsolute',
  '安装目录不能包含上级引用': 'config.installTargetParentRef',
  '仓库名不能为空': 'config.repositoryNameEmpty',
  '用户名不能为空': 'config.usernameEmpty',
  '仓库名不能包含路径分隔符': 'config.repositoryNameHasSeparator',

  // ===== 社区仓库 =====
  '社区搜索失败': 'community.searchFailed',
  '社区搜索超时': 'community.searchTimeout',
  '社区搜索被限流': 'community.rateLimited',
  'Gitee 社区搜索失败': 'community.giteeSearchFailed',
  'GitHub 社区搜索失败': 'community.githubSearchFailed',

  // ===== 头像与简介 =====
  '没有可用的头像下载源': 'avatar.noSource',
  '头像下载失败': 'avatar.downloadFailed',
  '头像缓存清理失败': 'avatar.clearFailed',
  '仓库简介读取失败': 'readme.readFailed',
  '仓库简介请求超时': 'readme.timeout',

  // ===== Wine =====
  '无法确定游戏使用的 Wine 容器，请重新选择厨房目录': 'wine.prefixUnknown',
  '请先安装 CrossOver，再启动 Wine 版厨房': 'wine.crossOverMissing',
  'Wine 容器路径无效': 'wine.prefixInvalid',
  'Wine 容器中未找到 Windows Steam，请先完成安装': 'wine.steamMissing',
  '无法自动配置 Wine winhttp，请在目标容器的 winecfg 中手动设置': 'wine.winhttpFailed',
  'Wine winhttp 自动配置超时，请在目标容器的 winecfg 中手动设置': 'wine.winhttpTimeout',

  // ===== 更新迁移 =====
  '安装目录无效': 'migrate.invalidInstallDirectory',
  '旧程序文件快照失败': 'migrate.snapshotFailed',
  '新版本未落盘': 'migrate.notInstalled',
  '新进程未持续运行': 'migrate.processNotStable',
  '无法读取当前运行目录': 'migrate.exeDirectoryUnavailable',

  // ===== 程序更新（补充）=====
  '更新签名公钥未配置': 'update.publicKeyMissing',
  '更新签名校验失败，已停止安装': 'update.signatureInvalid',
  '更新清单没有所选 Release 的更新版本': 'update.noNewVersion',

  // ===== 下载与归档（补充）=====
  '下载首字节等待超过 5 秒': 'download.firstByteTimeout',
  '服务器返回了错误的分段范围': 'download.badRangeResponse',
  '操作失败，部分文件保留在回收站，请查看日志': 'recycle.partialFailure',

  // ===== 框架（补充）=====
  '未找到原生 BepInEx 启动脚本，请先安装框架': 'framework.nativeScriptMissing',
  'winhttp.dll 与禁用文件同时存在，请先切换为框架已启用状态': 'framework.loaderConflict',
  '未找到框架加载器，请先安装框架': 'framework.loaderMissing',
  '原生 BepInEx 包缺少 run_bepinex.sh': 'framework.nativePackageMissingScript',
  '无法识别 macOS BepInEx 启动脚本，无法修复 Steam Overlay': 'framework.overlayScriptUnrecognized',
  '无法识别原生 BepInEx 启动脚本，请重新安装框架': 'framework.nativeScriptUnrecognized',

  // ===== 头像（补充）=====
  '头像需要仓库 owner': 'avatar.ownerRequired',
  '等待头像下载超时': 'avatar.waitTimeout',
  '头像请求超时': 'avatar.requestTimeout',
  '读取头像超时': 'avatar.readTimeout',
  '头像内容为空': 'avatar.emptyContent',
  '下载到的内容不是图片': 'avatar.notImage',
  '头像缓存路径无效': 'avatar.cachePathInvalid',

  // ===== 代理（补充）=====
  'GitHub 代理脚本中没有可识别的 Release 或 Raw 代理': 'proxy.noUsableEntries',
  'JS 文件没有可识别的 Release 或 Raw 代理': 'proxy.noUsableEntriesJs',
  '代理脚本缺少 raw_url 列表': 'proxy.missingRawList',
  'JS 文件缺少 download_url_us 或 raw_url 列表': 'proxy.missingDownloadListJs',

  // ===== 仓库刷新 =====
  '该仓库不在当前地区的 MOD 安装页展示，无法刷新': 'source.repositoryUnavailableInRegion',
  '该仓库已禁用，不会刷新': 'source.repositoryDisabled',
  '仓库已被删除': 'source.repositoryRemoved',
  '仓库配置已更改，请重新刷新': 'source.repositoryConfigChanged',
  '该仓库不在当前地区的 MOD 安装页展示，无法从这里安装': 'source.repositoryUnavailableForInstall',
  '没有可安装的 Release': 'source.noInstallableRelease',
  '没有匹配的 Release 资产': 'source.noMatchingAsset',
  '全局 GitHub 代理列表中没有可用的代理': 'proxy.noGlobalProxy',
  '请先安装 BepInEx 框架，再安装 MOD': 'framework.bepInExRequiredBeforeMods',
  'Steam Overlay 修复仅适用于 macOS 原生运行模式': 'framework.overlayMacNativeOnly',
  '尚未找到游戏目录': 'game.directoryMissing',
  '只能删除 BepInEx 时间戳备份': 'backup.wrongKindBepInEx',

  // ===== 回收站与导出（补充）=====
  'MOD 已从回收站恢复': 'recycle.modRestored',
  'MOD 已彻底删除': 'recycle.modPermanentlyDeleted',
  'MOD 包已从回收站恢复': 'recycle.packageRestored',
  'MOD 包已彻底删除': 'recycle.packagePermanentlyDeleted',
  '导出文件必须使用 .oc2repo.toml 或 .toml 扩展名': 'config.exportExtensionInvalid',
  '仓库 TOML 已复制到剪贴板': 'config.tomlCopied',
  '教程格式已复制': 'tutorial.formatCopied',
  '教程格式无效': 'tutorial.formatInvalid',

  // ===== 状态提示（无参数）=====
  '正在取消下载': 'download.cancelling',
  '已启用': 'mods.stateEnabled',
  '已禁用': 'mods.stateDisabled',

  // ===== 前端日志（后端拼接）=====
  '前端日志为空': 'log.frontendEmpty',

  // ===== 教程 =====
  '教程配置超过 1 MiB': 'tutorial.configTooLarge',
  '教程配置代码块未闭合': 'tutorial.unclosedCodeBlock',
  '教程 TOML 格式无效': 'tutorial.tomlInvalid',
  '导入文件超过 1 MiB': 'tutorial.importTooLarge',
  '教程步骤不能为空': 'tutorial.stepsEmpty',

  // ===== 仓库源编辑 =====
  '社区仓库源由自动搜索管理': 'source.communityManaged',
  '仓库源地址与配置不一致': 'source.addressMismatch',
  '此仓库源不支持内置回退': 'source.noBuiltinFallback',
  '仓库源昵称和地址不能为空': 'source.nicknameOrAddressEmpty',
  '仓库源昵称不能为空': 'source.nicknameEmpty',
  '仓库源地址已存在': 'source.addressExists',
  '内置仓库源不能修改': 'source.builtinCannotEdit',
  '缺少内置仓库源': 'source.builtinMissing',
  '仓库源必须使用 HTTP 或 HTTPS 地址': 'source.mustBeHttp',
  '仓库源标识重复': 'source.idDuplicate',
  '内置仓库源不能删除': 'source.builtinCannotDelete',

  // ===== 仓库编辑 =====
  '仓库唯一 ID 不能为空': 'config.repositoryIdEmpty',
  '仓库唯一 ID 格式无效': 'config.repositoryIdInvalid',
  '社区仓库由自动搜索管理，请导出后作为自定义仓库编辑': 'config.communityRepositoryReadOnly',
  '请创建至少一个有效的资产规则': 'config.assetRuleRequired',
  '版本号正则无效': 'config.versionRegexInvalid',
  '仓库名称不能为空': 'config.repositoryNameEmpty',
  '内置仓库不能删除，可禁用它': 'config.builtinRepositoryCannotDelete',
  '仓库填写不完整': 'config.repositoryIncomplete',

  // ===== 推送与状态 =====
  '已推送到街机MOD': 'rename.pushed',
  '当前模式下未找到厨房可执行文件': 'game.executableMissing',

  // ===== 框架状态与厨房操作 =====
  '框架状态已恢复': 'framework.stateRestored',
  'BepInEx 框架未安装，无法切换框架状态': 'framework.notInstalledCannotToggle',
  '框架已启用，尚未启动厨房': 'framework.enabledGameNotRunning',
  '框架已禁用，尚未启动厨房': 'framework.disabledGameNotRunning',
  '厨房当前未在运行': 'game.notRunning',
  '已停止厨房': 'game.stopped',
  '已启动厨房': 'game.started',

  // ===== 工具与文件定位 =====
  '开发者控制台已打开': 'tools.devConsoleOpened',
  '已清除缓存并重置界面': 'tools.cacheCleared',
  '已打开游戏目录': 'tools.gameDirOpened',
  '已打开模组目录': 'tools.modsDirOpened',
  '已打开备份目录': 'tools.backupDirOpened',
  '已打开目录并定位 MOD DLL': 'mods.fileLocationOpened',
  '已打开 MOD 日志目录': 'mods.logsDirOpened',
  '该 MOD 没有日志目录': 'mods.logsDirMissing',
  '只能定位 BepInEx/plugins 下的 MOD DLL 文件': 'mods.onlyPluginsLocatable',
  'MOD DLL 文件不存在，请重新扫描': 'mods.dllFileMissing',
  '无法识别 MOD 的 GUID，请重新扫描': 'mods.guidUnrecognized',
  '厨房目录已变化，请重新打开配置': 'game.directoryChangedReopenConfig',

  // ===== 发布页 =====
  '发布页必须使用 HTTPS': 'update.pageMustBeHttps',
  '发布页地址不能包含认证信息': 'update.pageHasCredentials',
  '已打开发布页': 'update.pageOpenedDetail',

  // ===== 仓库与状态（补充）=====
  '仓库已删除': 'config.repositoryDeleted',
  '文件格式无效': 'config.fileFormatInvalid',

  // ===== 权限与更新检查 =====
  '当前已是管理员权限': 'update.alreadyElevated',
  '未能获得管理员权限（可能取消了 UAC）': 'update.elevationFailed',
  '更新检查初始化失败，已跳过自动更新': 'update.checkInitFailed',
  '更新检查超时，已跳过自动更新': 'update.checkTimeout',
  '单个安装包下载超过 30 秒，已跳过当前渠道': 'update.downloadTimeout',
  '当前已是最新版本': 'update.alreadyLatest',
  '程序更新安装失败': 'update.installFailed',
  '程序更新安装失败，请手动更新': 'update.installFailedManual',
  '更新安装器已启动，请等待安装完成': 'update.installerStarted',
  '请手动更新管理器, 本次跳过更新.': 'update.manualUpdateSkipped',

  // ===== 安装与回滚（补充）=====
  '同一仓库绑定了多个旧 MOD 包，请先在 MOD 管理中确认要替换的包': 'mods.multipleOldPackages',
  '内置教程所需的主机实用 MOD 稳定版配置不存在': 'tutorial.hostUtilitiesConfigMissing',
  '内置教程所需的配置管理器配置不存在': 'tutorial.configManagerConfigMissing',

  // ===== 社区仓库 =====
  'GitHub 搜索触发速率限制，请稍后刷新': 'community.githubRateLimited',
  'Gitee 搜索触发速率限制，请稍后刷新': 'community.giteeRateLimited',
  '社区搜索没有返回结果': 'community.emptyResult',

  // ===== Wine（补充）=====
  '游戏使用的 Wine 容器不可用': 'wine.prefixUnavailable',

  // ===== 配置（补充）=====
  '配置目录不可用': 'config.directoryUnavailable',
  '配置解析失败': 'config.parseFailed',
  '配置写入失败': 'config.writeFailed',

  // ===== 直链下载 =====
  '下载的文件不是有效的 DLL': 'direct.notDll',
  '请输入有效的 HTTP 或 HTTPS 下载直链': 'direct.invalidUrl',
  '无法创建下载客户端': 'direct.clientFailed',
  '无法获取在线文件信息': 'direct.infoFailed',
  '无法创建安装临时目录': 'direct.tempDirFailed',
  '无法准备安装文件': 'direct.prepareFailed',
  '在线文件': 'direct.onlineFile',

  // ===== 下载（补充）=====
  '没有可用的下载地址': 'download.noCandidate',

  // ===== 游戏（补充）=====
  '所选厨房目录无效': 'game.invalidDirectory',
  '游戏进程仍在运行，请手动关闭': 'game.stillRunning',
  '路径必须是游戏目录内的相对路径': 'game.relativePathRequired',
  '游戏设置尚未初始化': 'game.settingsNotInitialized',
  '游戏状态尚未初始化': 'game.stateNotInitialized',

  // ===== 社区（补充）=====
  'Gitee 网页请求触发速率限制，请稍后刷新': 'community.giteeWebRateLimited',
  'Gitee 社区网页结构无效或需要验证，请稍后重试': 'community.giteeWebInvalid',
  'Gitee 社区网页地址无效': 'community.giteeWebUrlInvalid',
  'GitHub 社区搜索失败，请稍后重试': 'community.githubSearchFailed',
  'Gitee 社区搜索失败，请稍后重试': 'community.giteeSearchFailed',
  'Gitee 社区网页响应过大': 'community.giteeWebTooLarge',
  'Gitee 社区网页读取失败': 'community.giteeWebReadFailed',
  '无法创建 Gitee 验证浏览器': 'community.giteeBrowserFailed',
  '社区仓库搜索已取消': 'community.searchCancelled',
  'Gitee 网页验证超时，请稍后刷新': 'community.giteeVerifyTimeout',
  '无法显示 Gitee 验证浏览器': 'community.giteeBrowserShowFailed',

  // ===== 代理清单 =====
  'GitHub 代理清单超过 1 MiB': 'proxy.manifestTooLarge',
  'GitHub 代理脚本超过 4 MiB': 'proxy.scriptTooLarge',
  '代理 ID、名称和模板都不能为空': 'proxy.fieldsRequired',
  '代理模板包含未闭合的占位符': 'proxy.unclosedPlaceholder',
  '代理模板包含无效的大括号': 'proxy.invalidBraces',
  '其他': 'common.other',

  // ===== 配置（补充 2）=====
  '仓库源': 'config.labelSource',

  // ===== 代理模板（补充）=====
  'GitHub 代理模板必须是 HTTPS URL': 'proxy.templateMustBeHttps',

  // ===== 安装守卫 =====
  '管理器不能安装到 BepInEx 目录或其子目录，请选择游戏目录外的位置。': 'config.installGuardBepInEx',
  '管理器不能安裝到 BepInEx 目錄或其子目錄，請選擇遊戲目錄外的位置。': 'config.installGuardBepInExTraditional',

  // ===== MOD 扫描 =====
  '未知 MOD': 'mods.unknownName',
  '存在更高版本，建议卸载': 'mods.higherVersionPresent',

  // ===== MOD 配置编辑 =====
  'MOD 配置路径不安全': 'modConfig.unsafePath',
  'MOD 配置文件不存在或无法访问': 'modConfig.notFound',
  '无法读取 MOD 配置文件': 'modConfig.readFailed',
  'MOD 配置文件超过 2 MiB，无法编辑': 'modConfig.tooLarge',
  'MOD 配置文件不是有效的 UTF-8 文本': 'modConfig.invalidUtf8',
  '配置文件已被外部修改，请重新加载后再保存': 'modConfig.externallyModified',
  '无法保存 MOD 配置文件': 'modConfig.saveFailed',

  // ===== MOD 预览 =====
  '无法识别文件类型': 'modPreview.unknownType',
  '只支持 .dll 或 .zip 文件': 'modPreview.unsupportedType',
  '预览已过期，请重新选择文件': 'modPreview.expired',
  '预览标识无效': 'modPreview.invalidId',

  // ===== 回收站（补充 2）=====
  '回收站路径不安全': 'recycle.unsafePath',
  '回收站记录无效': 'recycle.invalidRecord',
  '回收站文件操作失败': 'recycle.fileOperationFailed',
  '恢复位置已有文件，拒绝覆盖': 'recycle.restoreTargetExists',
  '回收站文件不完整或已被修改': 'recycle.payloadModified',
  '没有可移入回收站的文件，安装清单保持不变': 'recycle.noMovableFiles',

  // ===== Releases =====
  '项目用户和仓库必须是安全的单一路径片段': 'source.coordinateUnsafe',
  '仓库文件模式只支持具体文件路径，不支持通配和正则': 'source.fileModeRequiresExactPath',
  '仓库文件分支或相对路径不安全': 'source.fileModePathUnsafe',
  '无法创建仓库文件地址': 'source.fileModeUrlFailed',
  '仓库文件路径缺少文件名': 'source.fileModeNameMissing',
  '仓库文件名编码无效': 'source.fileNameEncodingInvalid',
  '仓库文件名不是有效 UTF-8': 'source.fileNameNotUtf8',
  '仓库文件名不安全': 'source.fileNameUnsafe',
  'API 限流冷却中': 'source.apiRateLimited',
  '仓库默认分支缺失': 'source.defaultBranchMissing',
  'Release 响应不是数组或对象': 'source.releaseResponseInvalid',

  // ===== 仓库说明 =====
  '文件不存在': 'readme.fileMissing',
  '仓库说明请求标识无效': 'readme.invalidRequestId',
  '仓库说明请求过多，请稍后重试': 'readme.tooManyRequests',
  '正在读取仓库说明': 'readme.loading',
  '仓库说明超过 2 MiB，无法展示': 'readme.tooLarge',
  '仓库说明不是有效的 UTF-8 文本': 'readme.invalidUtf8',
  '无法确定仓库默认分支': 'readme.defaultBranchUnknown',
  '仓库说明读取已取消': 'readme.cancelled',

  // ===== 仓库源（补充）=====
  '仓库源路径不能超出当前仓库分支': 'source.pathEscapesBranch',
  'builtin 必须是 true 或 false': 'source.builtinFlagInvalid',
  'GitHub 代理来源': 'source.proxySourceLabel',

  // ===== 更新迁移（补充）=====
  '无法确定默认安装目录': 'migrate.defaultDirectoryUnknown',
  '迁移目录重叠，已停止更新': 'migrate.directoriesOverlap',
  '迁移数据包含目录链接，已保留原文件并停止更新': 'migrate.containsLinks',
  '原目录没有需要迁移的管理器': 'migrate.noManagerFiles',
  '原安装目录发生变化，已保留原文件': 'migrate.sourceChanged',
  '迁移路径或记录无效，已保留原文件': 'migrate.recordInvalid',
  '安装成功，但新版本未能保持运行，请手动启动并查看日志': 'migrate.newProcessNotStable',
  '迁移更新参数无效': 'migrate.invalidArguments',
  '无法读取安装后的程序版本': 'migrate.installedVersionUnreadable',
  '迁移后版本不符，已保留原管理器': 'migrate.versionMismatch',
  '更新日志目录无效': 'migrate.logDirectoryInvalid',
  '旧程序未能及时退出，未开始迁移': 'migrate.oldProcessStillRunning',
  '无法确定程序所在目录': 'migrate.exeDirectoryUnknown',

  // ===== 配置导入校验（补充 3）=====
  'format 不是 oc2-mod-repository-bundle': 'config.bundleFormatInvalid',
  '仓库缺少 name_i18n.zh-CN': 'config.missingNameI18n',
  '仓库的 name_i18n.zh-CN 不能为空': 'config.emptyNameI18n',
  '仓库缺少 group_i18n.zh-CN': 'config.missingGroupI18n',
  '仓库缺少 note_i18n.zh-CN': 'config.missingNoteI18n',
  '仓库文件模式的资产规则只支持具体文件': 'config.fileModeExactAssetsOnly',

  // ===== 社区搜索（补充）=====
  'GitHub 社区搜索地址无效': 'community.searchUrlInvalid',
  'GitHub 社区搜索响应过大': 'community.searchResponseTooLarge',
  'GitHub 社区搜索响应格式无效': 'community.searchResponseInvalid',
  '没有可用的 GitHub API 地址': 'community.noApiTarget',
  '社区仓库正在搜索': 'community.searchInProgress',
  'GitHub 搜索触发速率限制，请稍后刷新': 'community.searchRateLimited',
}

/**
 * 带参数的中文模板（需要配合占位符改名，不能逐个字符串替换）。
 */
export const TEMPLATE_ENTRIES = {
  '更新清单 HTTP {}': 'update.manifestHttpStatus',
  '更新清单过大': 'update.manifestTooLarge',
  '代理来源超过 {} MiB': 'proxy.sourceTooLargeByMib',
  '{} 个 MOD 已启用': 'message.mods.bulkToggleEnabled',
  '{} 个 MOD 已禁用': 'message.mods.bulkToggleDisabled',
  '切换到「{relative}」时失败：{error}': 'mods.toggleFailed',
  '未找到可回收的框架文件。备份已保存在 {}': 'backup.noReclaimableFiles',
  '文件回收失败，请尝试手动删除框架文件。（{error}）备份已保存在 {}': 'backup.recycleFailed',
  '文件回收失败，请尝试手动删除 DIYLevel 文件。（{error}）备份已保存在 {}': 'backup.diyLevelRecycleFailed',
  '{display_name} 安装完成': 'mods.installCompleted',
  '内置仓库源文件不存在: {path}': 'source.builtinFileMissing',
  '仓库源地址无效: {error}': 'source.addressInvalid',
  '代理来源地址无效: {error}': 'source.proxyAddressInvalid',
  '仓库源 TOML 解析失败: {error}': 'source.tomlInvalid',
  '不支持 GitHub 代理来源格式: {other}': 'proxy.unsupportedFormat',
  '清理旧 BepInEx 失败: {e}': 'framework.cleanupFailed',
  'BepInEx 安装失败: {error}': 'framework.installFailed',
  '名字代码超过 MOD 限制（UTF-16LE {} / {} 字节）': 'rename.nameTooLong',
  '当前程序版本无效: {e}': 'update.currentVersionInvalid',
  '下载文件落盘长度不完整: 文件为 {actual} 字节，实际收到 {downloaded} 字节': 'download.diskLengthMismatch',
  '服务器不支持分段下载 (HTTP {status})': 'download.rangeUnsupported',
  '分片下载落盘长度不完整: 文件为 {actual_size} 字节，预期 {total} 字节': 'download.chunkDiskLengthMismatch',
  '头像超过大小上限：{} 字节': 'avatar.tooLarge',
  'ZIP 顶层目录不是预期的 {root}: {}': 'archive.unexpectedRootDirectory',
  '复制 MOD 文件失败: {error}；恢复旧文件失败: {restore_error}': 'archive.copyAndRestoreFailed',
  'ZIP 读取失败: {e}': 'archive.zipReadFailed',
  'ZIP 条目读取失败: {e}': 'archive.zipEntryReadFailed',
  'ZIP 内容校验失败: {e}': 'archive.zipValidationFailed',
  '无法刷新「{}」，仓库文件模式下无法探测版本号': 'source.fileModeNoVersion',
  '{} 已就绪': 'mods.ready',
  '下载期间厨房重新启动，安装已取消，旧版本未删除': 'mods.gameRestartedDuringDownload',
  '{} 安装完成': 'mods.installCompletedSimple',
  '{} {} 安装完成': 'mods.installCompletedWithVersion',
  '已清空回收站（{removed} 条记录）': 'recycle.emptied',
  '已导出到 {path}': 'config.exported',
  '创建 HTTP 客户端失败: {e}': 'network.clientCreateFailed',
  '{}...[前端日志已截断]': 'log.frontendTruncated',
  '超时={}，连接错误={}，原因={}': 'proxy.downloadFailureDetail',
  '已读取字节数={}，超时={}，原因={}': 'proxy.readFailureDetail',
  '教程 TOML 格式无效：{error}': 'tutorial.tomlInvalidReason',
  '不支持的教程格式版本：{}': 'tutorial.unsupportedVersion',
  '教程 TOML 导出失败：{error}': 'tutorial.exportFailed',
  '仓库源 {}': 'source.labeled',
  '仓库 {}': 'config.repositoryLabeled',
  '发布页地址无效: {e}': 'update.pageInvalid',
  '当前程序版本无效: {error}': 'update.currentVersionInvalid2',
  '文件回收失败，请尝试手动删除 mod。（{error}）': 'mods.recycleFailed',
  '安装失败，已尝试回滚: {error}': 'mods.installFailedRolledBack',
  '目录不存在: {}': 'mods.directoryMissing',
  '正在下载程序更新': 'update.downloadingInstaller',
  '{name} 社区仓库': 'community.labelSimplified',
  '{name} 社群倉庫': 'community.labelTraditional',
  '{} 的平台无效：{value}（可选 windows 或 macos，留空表示全平台）': 'config.invalidPlatform',
  '配置目录不可用: {0}': 'config.directoryUnavailableReason',
  '配置读取失败: {0}': 'config.readFailedReason',
  '配置解析失败: {0}': 'config.parseFailedReason',
  '配置写入失败: {0}': 'config.writeFailedReason',
  '配置格式无效: {0}': 'config.invalidFormatReason',
  'GitHub 代理清单解析失败: {error}': 'proxy.manifestParseFailed',
  '不支持 GitHub 代理清单版本 {}': 'proxy.unsupportedManifestVersion',
  '每类代理最多允许 {MAX_PROXY_COUNT} 条': 'proxy.tooManyEntries',
  '代理脚本缺少 {variable} 列表': 'proxy.missingVariableList',
  '{variable} 列表不完整': 'proxy.incompleteVariableList',
  '{variable} 列表没有闭合': 'proxy.unclosedVariableList',
  '内置 {kind} 代理不可删除或移动到其他类型: {missing}': 'proxy.builtinImmutable',
  '代理模板占位符不受支持: {{{token}}}': 'proxy.unsupportedPlaceholder',
  '代理模板 URL 无效: {error}': 'proxy.templateUrlInvalid',
  'GitHub 代理 ID 重复: {}': 'proxy.duplicateId',
  'ZIP 读取失败: {error}': 'archive.zipReadFailed2',
  'ZIP 条目读取失败: {error}': 'archive.zipEntryReadFailed2',
  '拒绝不安全 ZIP 路径: {}': 'archive.unsafePath2',
  '仓库说明下载失败：HTTP {status}': 'readme.httpStatus',
  '没有为地区 {region} 配置 GitHub 代理清单地址': 'source.proxyManifestMissing',
  '仓库源标识重复: {}': 'source.idDuplicateValue',
  '无法启动更新后的程序: {error}': 'migrate.launchFailed',

  // ===== 配置导入 / 校验（补充 3）=====
  'TOML 解析失败: {e}': 'config.tomlParseFailed',
  '不支持 schema_version {}': 'config.unsupportedSchemaVersion',
  '{} (导入)': 'config.importedNameSuffix',
  '{} 的项目主页无效: {error}': 'config.homepageInvalid',
  '{} 的安装目录不安全': 'config.installTargetUnsafe',
  '{} 的版本号正则无效': 'config.versionRegexInvalid',
  '{} 的 release_relations 必须至少指定 min_version 或 max_version': 'config.relationsEmpty',
  '{} 的 release_relations {label} 无法解析：{value}': 'config.relationsValueInvalid',
  '{} 的 release_relations 区间无效：min_version {min} 大于 max_version {max}': 'config.relationsRangeInvalid',
  '{repository_name} 的 {kind} 关系缺少 GUID': 'config.relationMissingGuid',
  '{repository_name} 的 {kind} 关系 {} 不能标记 optional：只有 requires 支持可选': 'config.relationOptionalUnsupported',
  '{repository_name} 的 requires 关系 {} 不能限定 prerelease：依赖版本请用版本区间表达': 'config.relationRequiresPrerelease',
  '{repository_name} 的 {kind} 关系指向自己的 GUID，必须用 prerelease = true/false 区分': 'config.relationSelfGuid',
  '{repository_name} 的 {kind} 关系 {} {label} 无法解析：{value}': 'config.relationValueInvalid',
  '{repository_name} 的 {kind} 关系 {} 版本区间无效：min_version {min} 大于 max_version {max}': 'config.relationRangeInvalid',

  // ===== 回收站（补充 3）=====
  '清空回收站时在第 {} 条失败（已删除 {} 条）：{error}': 'recycle.emptyFailed',

  // ===== 社区（补充 2）=====
  'GitHub 社区搜索 HTTP {status}': 'community.searchHttpStatus',
}

/** 全部条目（普通 + 模板），供测试与脚本共用。 */
export const ALL_ENTRIES = { ...ENTRIES, ...TEMPLATE_ENTRIES }

/**
 * 模板 tag 的参数名（按原 `format!` 里的占位符顺序）。
 *
 * `apply-backend-tags.mjs` 用它把 `format!("tag", a, b)` 自动改写成
 * `locale_tag::with_params("tag", &[("a", ...), ("b", ...)])`。
 *
 * 参数名同时决定前端插值用的 `{name}`：两边必须一致，所以这张表与
 * `src/locale/*.ts` 里对应条目的占位符必须同步（有测试核对）。
 */
export const TEMPLATE_PARAMS = {
  'archive.unexpectedRootDirectory': ['root', 'entry'],
  'avatar.tooLarge': ['bytes'],
  'update.manifestHttpStatus': ['status'],
  'update.manifestTooLarge': ['bytes'],
  'proxy.sourceTooLargeByMib': ['mib'],
  'proxy.sourceTooLarge': ['bytes'],
  'message.mods.bulkToggleEnabled': ['count'],
  'message.mods.bulkToggleDisabled': ['count'],
  'mods.toggleFailed': ['relative', 'error'],
  'backup.noReclaimableFiles': ['path'],
  'backup.recycleFailed': ['reason', 'path'],
  'backup.diyLevelRecycleFailed': ['reason', 'path'],
  'mods.installCompleted': ['name'],
  'source.builtinFileMissing': ['path'],
  'source.addressInvalid': ['reason'],
  'source.proxyAddressInvalid': ['reason'],
  'source.tomlInvalid': ['reason'],
  'proxy.unsupportedFormat': ['format'],
  'framework.cleanupFailed': ['reason'],
  'framework.installFailed': ['reason'],
  'rename.nameTooLong': ['bytes', 'limit'],
  'update.currentVersionInvalid': ['reason'],
  'update.currentVersionInvalid2': ['reason'],
  'download.diskLengthMismatch': ['actual', 'downloaded'],
  'download.rangeUnsupported': ['status'],
  'download.chunkDiskLengthMismatch': ['actual', 'total'],
  'archive.copyAndRestoreFailed': ['error', 'restore'],
  'archive.zipReadFailed': ['reason'],
  'archive.zipReadFailed2': ['reason'],
  'archive.zipEntryReadFailed': ['reason'],
  'archive.zipEntryReadFailed2': ['reason'],
  'archive.zipValidationFailed': ['reason'],
  'archive.unsafePath': ['entry'],
  'archive.unsafePath2': ['entry'],
  'source.fileModeNoVersion': ['repo'],
  'mods.ready': ['name'],
  'mods.installCompletedSimple': ['name'],
  'mods.installCompletedWithVersion': ['name', 'version'],
  'mods.recycleFailed': ['reason'],
  'mods.installFailedRolledBack': ['reason'],
  'mods.directoryMissing': ['path'],
  'recycle.emptied': ['count'],
  'config.exported': ['path'],
  'config.invalidPlatform': ['repo', 'value'],
  'config.directoryUnavailableReason': ['reason'],
  'config.readFailedReason': ['reason'],
  'config.parseFailedReason': ['reason'],
  'config.writeFailedReason': ['reason'],
  'config.invalidFormatReason': ['reason'],
  'proxy.manifestParseFailed': ['reason'],
  'proxy.unsupportedManifestVersion': ['version'],
  'proxy.tooManyEntries': ['max'],
  'proxy.missingVariableList': ['variable'],
  'proxy.incompleteVariableList': ['variable'],
  'proxy.unclosedVariableList': ['variable'],
  'proxy.builtinImmutable': ['kind', 'missing'],
  'proxy.unsupportedPlaceholder': ['token'],
  'proxy.templateUrlInvalid': ['reason'],
  'proxy.duplicateId': ['id'],
  'tutorial.tomlInvalidReason': ['reason'],
  'tutorial.unsupportedVersion': ['version'],
  'tutorial.exportFailed': ['reason'],
  'source.labeled': ['label'],
  'config.repositoryLabeled': ['name'],
  'readme.httpStatus': ['status'],
  'source.proxyManifestMissing': ['region'],
  'source.idDuplicateValue': ['id'],
  'migrate.launchFailed': ['reason'],
  'update.pageInvalid': ['reason'],
  'network.clientCreateFailed': ['reason'],
  'log.frontendTruncated': ['text'],
  'proxy.downloadFailureDetail': ['timeout', 'connect', 'reason'],
  'proxy.readFailureDetail': ['bytes', 'timeout', 'reason'],
  'update.downloadingInstaller': [],
  'community.labelSimplified': ['name'],
  'community.labelTraditional': ['name'],
  'download.noCandidate': [],

  // 补充 3
  'config.tomlParseFailed': ['reason'],
  'config.unsupportedSchemaVersion': ['version'],
  'config.importedNameSuffix': ['name'],
  'config.homepageInvalid': ['repo', 'reason'],
  'config.installTargetUnsafe': ['repo'],
  'config.versionRegexInvalid': ['repo'],
  'config.relationsEmpty': ['repo'],
  'config.relationsValueInvalid': ['repo', 'label', 'value'],
  'config.relationsRangeInvalid': ['repo', 'min', 'max'],
  'config.relationMissingGuid': ['repo', 'kind'],
  'config.relationOptionalUnsupported': ['repo', 'kind', 'guid'],
  'config.relationRequiresPrerelease': ['repo', 'guid'],
  'config.relationSelfGuid': ['repo', 'kind'],
  'config.relationValueInvalid': ['repo', 'kind', 'guid', 'label', 'value'],
  'config.relationRangeInvalid': ['repo', 'kind', 'guid', 'min', 'max'],
  'recycle.emptyFailed': ['index', 'removed', 'reason'],
  'community.searchHttpStatus': ['status'],
}
