export type * from './diychef/types'
export type * from './diychef/preview/types'
export type RequestPlanStep = { kind: 'official' | 'direct' } | { kind: 'proxy'; id: string }
export interface RequestRouteOverride {
  route?: RequestPlanStep[]
  race?: boolean
  max_parallelism?: number
  download_threads?: number
}
export type RequestPlanReference = string | {
  id?: string
  api?: RequestRouteOverride
  release?: RequestRouteOverride
  raw?: RequestRouteOverride
}
export interface NetworkPlanSelection {
  mainland?: RequestPlanReference
  international?: RequestPlanReference
}

export type Provider = 'github' | 'gitee' | 'gitcode'
/**
 * 仓库支持的桌面平台。
 *
 * 只发布 Windows 与 macOS 两个平台；空值 / 缺失表示全平台可用。
 */
export type RepositoryPlatform = 'windows' | 'macos' | ''
/**
 * 桌面平台。只保留 Windows 与 macOS —— 本项目不再发布 Linux 版本。
 */
export type AppPlatform = 'windows' | 'macos'
export type GameRuntime = 'wine' | 'native'
export type Channel = 'stable' | 'beta' | 'all'
export type ProxyMode = 'direct' | 'auto_direct' | 'auto_proxy' | 'fixed'
export type ModDownloadMode = 'release' | 'repository_file'
export type AssetRuleType = 'exact' | 'glob' | 'regex'
export interface GitHubProxy { id: string; name: string; template: string; regions: string[]; description: string; enabled: boolean; builtin: boolean; remote: boolean }
export interface GitHubProxyConfig {
  release_proxies: GitHubProxy[]
  raw_proxies: GitHubProxy[]
  github_api_proxies: GitHubProxy[]
  gitee_api_proxies: GitHubProxy[]
  gitcode_api_proxies: GitHubProxy[]
  /** 头像专用代理：能服务任意 URL 的那类（纯 `{url}` 包装）。 */
  avatar_proxies: GitHubProxy[]
}
export interface AssetRule { type: AssetRuleType; value: string }
export interface ModRepositoryBinding { provider: Provider; owner: string; repository: string; tag: string; channel: Channel }
export interface CommunityDiscovery { repository_ids: string[]; total_count: number; incomplete_results: boolean; truncated: boolean }
export interface RepositoryReadme { content: string; exists: boolean; url: string }
export interface ReleaseAsset { name: string; size: number; download_count?: number | null; download_url: string; digest?: string }
export interface ReleaseInfo {
  release_id?: number
  tag_name: string
  version?: string
  name: string
  name_i18n?: Record<string, string>
  body: string
  published_at?: string
  prerelease: boolean
  draft: boolean
  assets: ReleaseAsset[]
}
/**
 * MOD 与另一个插件之间的关系种类。
 *
 * 依赖、排斥、兼容三件事本质上是同一个问题 ——「对某个 GUID 的版本区间
 * 有什么要求」，只是满足与违反时的含义不同：
 *
 * - `requires`   目标必须存在且版本落在区间内；否则是「缺依赖」
 * - `conflicts`  目标存在且版本落在区间内 → 冲突，需要卸载
 * - `compatible` 目标存在时版本必须落在区间内；否则不兼容，需要卸载
 *
 * 三者共用 `version`：不写（或写 `*`）就是不限版本。
 */
export type RelationKind = 'requires' | 'conflicts' | 'compatible'

/** 一条关系声明：对某个 GUID 的版本要求（pip 风格区间表达式）。 */
export interface RepositoryRelation {
  /** 省略时按 `requires` 处理（历史写法只写 guid）。 */
  kind?: RelationKind
  guid: string
  /**
   * 版本要求，pip 风格区间表达式：`>=1.8.0,<2.0.0`、`==1.2.3`、`!=1.0`…
   *
   * 空字符串（或缺省）表示不限版本。判定统一走 `versionSpec.ts` 的
   * `versionMatches`，前端不再有第二套区间比较。
   */
  version?: string
  /**
   * 只对预发布版本成立（三态）。
   *
   * - 不写：正式版与预发布版都算
   * - `true`：只匹配预发布版（四段写法 `1.9.8.1` 即 `1.9.8-beta.1`）
   * - `false`：只匹配正式版
   *
   * 用于「正式版与测试版互斥」：预发布版按 semver 排在**同号正式版之下**，
   * 版本区间无法把 `1.9.8` 和 `1.9.8.1` 分成两类，只能靠这个标志。
   */
  prerelease?: boolean
  /** 仅对 `requires` 有意义：缺失时不提示、不显示按钮。 */
  optional?: boolean
}

/**
 * 按 Release 版本覆盖关系表。
 *
 * 同一 MOD 的不同大版本可以依赖不同版本的基础库。匹配规则：
 * 所有区间命中的条目里取**下限最高**的那条，其 `relations`
 * 整体替换仓库级 `relations`（不合并）。
 */
export interface ReleaseRelationOverride {
  /**
   * 这条覆盖表适用的 Release 版本（pip 风格区间表达式）。
   *
   * 必填且必须能解析：空表达式等于匹配一切，会让下面每条覆盖规则都
   * 生效，配置错误不该被静默当成「全版本」。
   */
  version: string
  relations: RepositoryRelation[]
}

export interface ModRepository {
  id: string
  name: string
  name_i18n?: Record<string, string>
  group?: string
  group_i18n?: Record<string, string>
  provider: Provider
  username: string
  repository: string
  download_mode: ModDownloadMode
  branch: string
  file_path: string
  region?: string
  /** 支持的桌面平台；空 / 缺失表示全平台可用（与 region 同语义层）。 */
  platform?: RepositoryPlatform
  channel: Channel
  asset_rules: AssetRule[]
  install_target: string
  note: string
  note_i18n?: Record<string, string>
  source_id?: string
  plugin_guid: string
  local_product_name: string
  local_original_filename: string
  /** 这个 MOD 与其它插件的关系（依赖 / 排斥 / 兼容，按 GUID 声明）。 */
  relations?: RepositoryRelation[]
  /** 按 Release 版本覆盖关系表（见 ReleaseRelationOverride）。 */
  release_relations?: ReleaseRelationOverride[]
  version_regex: string
  max_release_count: number
  /** Legacy input only. Downloads use the resolved request plan. */
  download_threads?: number
  type?: 'mod' | 'map'
  network_plan?: NetworkPlanSelection
  github_token_set?: boolean
  gitee_token_set?: boolean
  enabled: boolean
  builtin: boolean
  pinned_version?: string
  ignored_version?: string
  cached_releases: ReleaseInfo[]
  last_error?: string
}
export interface PublicConfig {
  schema_version: number
  platform: AppPlatform
  region: string
  ui_zoom: number
  hide_unidentified_mods: boolean
  /** 删除 MOD 时先移入回收站；false = 直接删除，不可恢复。 */
  recycle_deleted_mods: boolean
  /**
   * 读取 MOD DLL 元数据的并发上限：0 = 全部 CPU 核心。
   *
   * 落盘值原样保存（允许 0，也允许超过当前机器核心数 —— 配置会跟着用户换机器），
   * 真正使用时由后端 `metadata_read::resolve_concurrency` 解析成 `clamp(1, cores)`。
   * 设置页下拉同理：只按本机核心数 clamp 显示，不回写。
   */
  mod_metadata_concurrency: number
  /** 解析地图 `info` 的并发上限，语义同 `mod_metadata_concurrency`；默认 0（全部核心）。 */
  map_info_concurrency: number
  /** 后台 LZMA 转 LZ4 并发上限；0 = 全部核心，独立于 info 读取。 */
  map_compression_concurrency: number
  /** 暖暖缩略图并行准备上限；0 = 全部 CPU 核心。 */
  diychef_preview_concurrency: number
  /** 已经看过的教程步骤 id；启动时只播不在里面的步骤。 */
  tutorial_watched_steps?: string[]
  game: { path: string; runtime: GameRuntime }
  network: { timeout_seconds: number; download_threads: number; max_retry_count: number }
  launch: { ask_user_to_start_from_steam: boolean; auto_restore_winhttp: boolean }
  official_api_only: boolean
  github_token_set: boolean
  gitee_token_set: boolean
  gitcode_token_set: boolean
  repositories: ModRepository[]
  repository_sources: RepositorySource[]
  community_discovery?: CommunityDiscovery | null
  gitee_community_discovery?: CommunityDiscovery
  gitcode_community_discovery?: CommunityDiscovery | null
  github_proxies: GitHubProxyConfig
  /** 本次启动的数据来源；下次启动重新按电脑地区初始化。 */
  data_provider: DataProvider
}
export interface RepositorySource {
  network_plan?: NetworkPlanSelection
  id: string; nickname: string; nickname_i18n?: Record<string, string>; url: string; region?: string; builtin: boolean; parent_id?: string }
export interface BepInExInfo { installed: boolean; has_residue: boolean; has_config: boolean; version?: string; version_source?: string; backups: string[] }
/**
 * DIYLevel 自定义关卡框架状态。
 *
 * 它本质是一个 MOD（GUID `dev.gua.overcooked.diylevel`），但承担「框架」角色，
 * 所以在设置页有独立管理区块。判定用 GUID，不看文件名。
 */
export interface DiyLevelInfo {
  installed: boolean
  enabled: boolean
  version?: string | null
  dll_file?: string | null
  /** 地图目录 `BepInEx/plugins/OC2DIYLevel/levels` 是否存在。 */
  has_levels_dir: boolean
  /** 地图集数量：只数 `levels/` 下的文件夹（散文件不算）。 */
  level_packs: number
  /** 已有的 DIYLevel 目录备份（绝对路径，升序）。 */
  backups?: string[]
}
export type WinhttpState = 'modenabled' | 'moddisabled' | 'missing' | 'conflict'

/**
 * 一个 DIYLevel 关卡集（玩家口中的「地图」）。
 *
 * 与 `InstalledMod` 是两套数据：MOD 靠 BepInPlugin 的 GUID 识别、由 BepInEx
 * 加载；关卡集是 `levels/` 下的一个目录、由 DIYLevel 读取。界面复用同一套
 * 卡片样式，模型刻意不合并 —— 合并后每个使用点都要判「这是哪种」。
 */
export interface LevelSetCompressionProgress {
  running: boolean
  sets_done: number
  sets_total: number
  set_name: string
  files_done: number
  files_total: number
  done: number
  total: number
  converted: number
  skipped: number
  failed: number
  percent: number
  current: string
}
export interface LevelSetScanProgress {
  /** 已经解析完的份数。 */
  done: number
  /** 本次要解析的总份数（扫描开始时一次列目录得到）。 */
  total: number
  /** 正在读取的关卡集目录名。 */
  current: string
}

/** 「一键拍平」探测到的一个候选目录。 */
export interface LevelSetFlattenCandidate {
  /** `levels/` 下的直接子目录名。 */
  dir_name: string
  /** 浅探测（≤3 层）找到的 `info` / `info_*` 文件数。 */
  nested_infos: number
}

/**
 * 「一键拍平」的结果。
 *
 * `dry_run` 只用来探测（不写盘，也不要求游戏关闭），此时 `candidates` 就是全部
 * 结果；正式整理时 `flattened` / `failed` 才有值。
 */
export interface LevelSetFlattenReport {
  candidates: LevelSetFlattenCandidate[]
  flattened: number
  /** 逐条失败记录，形如 `"<目录>: <原因>"`，原样展示给用户。 */
  failed: string[]
}

/**
 * 来源 ZIP 的识别 / 导入进度（后端 `level-set-zip-progress`）。
 *
 * 与 `LevelSetScanProgress` 的区别是读的对象：这一份读的是**来源 ZIP**
 * （本地拖入 / 直链 / 在线 Release 下载来的），暂存阶段要识别里面的
 * `info`，导入阶段要把它解压落盘。一个上百 MB 的合集在 debug 构建里要
 * 几十秒，只转圈不报进度，用户分不清是在跑还是卡死。
 */
export interface LevelSetZipProgress {
  /** `info` = 暂存阶段解析 ZIP 里的 info；`install` = 导入阶段写盘。 */
  phase: 'info' | 'install'
  /** 已完成数：`info` 阶段是已解析的目录数，`install` 阶段是已写入的关卡集数。 */
  done: number
  total: number
  /** 正在处理的条目：`info` 阶段是 ZIP 内条目名，`install` 阶段是关卡集名。 */
  current: string
}

export interface LevelSetInfo {
  /** 目录名（唯一键，也是开关时移动的目录名）。 */
  dir_name: string
  /**
   * 关卡集目录的绝对路径。
   *
   * 开关会在 `levels/` 与 `BepInEx/cache/modmanager_disabled_maps/` 之间移动整个目录，所以用户要知道的
   * 不只是目录名，而是「它现在到底在磁盘哪里」。由后端按真实数据根给出，
   * 前端不再自己拼游戏目录。
   */
  path: string
  /** `true` = 在 `levels/`；`false` = 在 `BepInEx/cache/modmanager_disabled_maps/`。 */
  enabled: boolean
  /** info 里的英文集名；解析失败时为空。 */
  name: string
  name_zh?: string | null
  /** info 里的作者；官方导出常为空。 */
  author: string
  /** 地图自己的版本号（与 Release tag 比对）。 */
  version: string
  /**
   * 安装/更新那一刻的资产版本（Release 的 tag_name，如 `3.4.1-fix`）。
   *
   * 作者发修复版时常忘记改 info 里的版本号（`3.4.1-fix` 与 `3.4.1` 的 `version`
   * 都是 `3.4.1`），只比 info 版本分不出装的是不是最新资产，所以安装时把当时的
   * 资产版本记进 `map-relations.json` 快照，扫描时按身份回填到这里。
   * 本地 ZIP 导入或老版本装的没有记录，为空串。
   */
  installed_mapset_version?: string
  /** MD5(baseUID + version)：随版本变化，不能当稳定身份。 */
  uid: string
  /** 稳定身份（老导出可能为空）。 */
  base_uid?: string | null
  /** 关卡数（按 info 里 levelInfos 的声明统计）。 */
  level_count: number
  /** info 内逐关元数据；兼容旧扫描结果。 */
  levels?: { name: string; name_zh?: string | null; scene_name: string; min_order_count: number; max_order_count: number }[]
  /** 解析失败的原因（tag）；成功时无此字段。 */
  error?: string | null
  /**
   * 绑定的地图仓库 id（按 `level_set_name + level_set_name_zh + author` 身份匹配）。
   *
   * 后端从配置里算出来：前端本来要自己 `mapForLevelSet` 一遍，但「这个集
   * 有没有线上条目」在依赖表、删除确认、跳转三处都要用，交给后端一次算清
   * 比每个使用点各匹配一次更不容易漂移。未绑定时为空。
   */
  map_id?: string | null
  /**
   * 线上声明的分组多语言表（地图的分组语义就是作者）。
   *
   * 后端按「绑定地图的当前声明 → 安装时固化的 `map-relations.json` 快照」
   * 填进来：地图没有 MOD 那样的安装清单，源文档被删或一直刷新失败时分组
   * 就无处可查，而管理页的折叠分组与详情里的作者中英两名都要靠它稳定。
   * 两者都没有时前端回退 info 里的本地 `author`。
   */
  map_group_i18n?: Record<string, string>
  /** 分组的单语镜像（旧源文档只写 `group` 时用它）。 */
  map_group?: string | null
  /**
   * 依赖声明快照（来自地图依赖清单）。
   *
   * 在线地图条目还在时以在线为准（见 `dependencyRows.ts`）；这条只在仓库
   * 下线/被删之后兜底，否则地图管理页的依赖表会凭空消失。
   */
  relations?: RepositoryRelation[]
  /**
   * 目录里随地图一起落盘的插件 DLL（作者随包的修复/运行库）。
   *
   * 它们是这个地图集的一部分：与地图同处一个父目录、跟着整目录一起启停，
   * 不进本地 MOD 列表（否则同一份文件会有「MOD 开关」和「地图开关」两套状态）。
   */
  plugins?: LevelSetPlugin[]
}

/**
 * 安装前扫描到的同身份已安装项。
 *
 * MOD 以 DLL 文件名为身份、地图以关卡集身份（名称 + 中文名 + 作者）匹配；
 * 有冲突时必须先卸载（进回收站）再安装，`same_version` 只决定提示文案，
 * 不改变「先卸载再安装」这个动作。
 */
export interface InstallConflict {
  /** 展示名：MOD 的 BepInPlugin 名，或关卡集的显示名。 */
  name: string
  version: string
  /** 卸载句柄：MOD 是 DLL 文件名，地图是关卡集目录名。 */
  handle: string
  /** 已安装项当前是否启用（地图开关是整目录 rename，MOD 是 `.disabled` 后缀）。 */
  enabled: boolean
  /** 已安装版本与本次安装版本相同。 */
  same_version: boolean
}

/** 在线暂存实际选中的资产版本，只属于声明的主地图。 */
export interface MapInstallOrigin {
  level_set_name: string
  level_set_name_zh: string
  author: string
  mapset_version: string
}

/**
 * 后端暂存好的一份地图来源（`stage_map_source` / `stage_map_release` 的返回）。
 *
 * `path` 是后端能读到的 ZIP 绝对路径，`sets` 是里面识别出的关卡集。
 * 在线来源的 `origin` 由后端确定，确认导入时原样回传。
 */
export interface StagedMapSource {
  /** ZIP 绝对路径（`staged` 为真时位于后端 cache 目录）。 */
  path: string
  /**
   * `true` = 后端为这次安装下载/复制到 cache 的临时文件，导入结束必须
   * `discard_map_source` 清理；`false` = 用户自己的本地文件，不能删。
   */
  staged: boolean
  /** ZIP 里识别出的关卡集（空数组 = 不是有效关卡集包）。 */
  sets: ZipLevelSet[]
  origin?: MapInstallOrigin | null
}

/**
 * 一个 DLL 的元数据（BepInPlugin + PE 版本资源）。
 *
 * MOD 预览、ZIP 随包插件、已安装地图集里的插件都用这一份：后端只有
 * `mods::read_dll_details` 一个解析实现，字段也只有一个 `DllPluginInfo`。
 */
export interface DllPluginInfo {
  /** 读不出 BepInPlugin 也没有版本号时是 dependency（纯依赖库，不是插件）。 */
  role: ModPreviewRole
  name?: string | null
  guid?: string | null
  version?: string | null
  title?: string | null
  product_name?: string | null
  company?: string | null
  original_filename?: string | null
}

/**
 * ZIP 里随某个关卡集一起落盘的插件 DLL。
 *
 * 和 `ModPreviewEntry` 是同一套元数据、同一套读取规则（后端只有
 * `mods::read_dll_details` 一份实现），差别只在来源：一个是用户在装 MOD，
 * 一个是地图作者随包带的修复插件 —— 界面上两者都用同一张详情卡片展示。
 */
export interface ZipLevelSetPlugin extends DllPluginInfo {
  /** ZIP 内的条目路径（展示用，例如 `OC2DIYLevel/OC2DIYLevel.dll`）。 */
  path: string
  /** 落盘后的文件名：插件会被复制进关卡集目录根部。 */
  file_name: string
  /**
   * 落盘位置（相对游戏目录，正斜杠）。
   *
   * 只有 BepInEx 表层包会带：那种包已经规划好目录，插件**不**搬进集目录，
   * 而是留在 ZIP 指的 `BepInEx/plugins/**` 下，靠安装清单登记归属。拍平形态
   * 没有这个值 —— 位置由集目录决定。
   */
  install_path?: string | null
}

/**
 * 已安装关卡集目录里的一个插件 DLL。
 *
 * 与 `ZipLevelSetPlugin` 的区别：它已经在磁盘上，所以没有 ZIP 内路径，
 * 多一个 `enabled`（`.dll.disabled` 记为未启用）。地图开关是整目录 rename，
 * 所以正常情况下它跟着地图一起启停。
 */
export interface LevelSetPlugin extends DllPluginInfo {
  file_name: string
  enabled: boolean
}

/** 关卡集目录的落盘形态（与后端 `LevelSetLayout` 同名字面量）。 */
export type LevelSetLayout = 'flattened' | 'bepinex'

/** ZIP 里识别出的一个关卡集（导入前确认用）。 */
export interface ZipLevelSet {
  dir_name: string
  /**
   * 落盘形态。
   *
   * `flattened`（历史形态）：管理器把每个集拍平成 `levels/<集名>/`，随包插件
   * 也搬进集目录根。`bepinex`：作者已经把目录规划好（ZIP 顶层就是 `BepInEx/`），
   * 按包内路径原样解压，插件留在 `BepInEx/plugins/**`。
   */
  layout?: LevelSetLayout
  name: string
  name_zh?: string | null
  author: string
  version: string
  level_count: number
  file_count: number
  /** 目标位置已安装的同身份关卡集；非空时必须先卸载再导入。 */
  conflicts: InstallConflict[]
  /** 随这个关卡集一起装进集目录的插件（作者随包的修复 DLL）。 */
  plugins: ZipLevelSetPlugin[]
}

export interface GameInfo { path?: string; steam_library?: string; steam_root?: string; wine_prefix?: string; game_exists: boolean; bepinex: BepInExInfo; kitchen_running: boolean; winhttp_state: WinhttpState }
export type InstalledModStatusLabel = 'update_available' | 'latest' | 'not_release_asset' | 'unbound' | 'bound_repository_missing' | 'repository_unavailable' | 'refreshing' | 'unknown' | 'duplicate' | 'duplicate_lower_version' | 'load_warning' | 'disabled' | 'dependency' | 'missing_dependency' | 'dependency_conflict' | 'dependency_installable'
export interface InstalledMod {
  repository_id?: string
  repository_ids?: string[]
  repository_binding?: ModRepositoryBinding | null
  package_id?: string
  package_name?: string
  package_name_i18n?: Record<string, string>
  package_mod_count?: number
  group_i18n?: Record<string, string>
  group?: string
  prerelease?: boolean
  name: string
  title?: string
  description?: string
  product_name?: string
  original_filename?: string
  guid?: string
  has_config?: boolean
  has_logs?: boolean
  plugin_name?: string
  manual_install?: boolean
  is_dependency?: boolean
  company?: string
  copyright?: string
  dll_file: string
  enabled: boolean
  version?: string
  version_source: string
  update_available: boolean
  labels?: InstalledModStatusLabel[]
  duplicate_guid: boolean
  duplicate_version?: boolean
  load_warning?: string
  note?: string
  status: string
  /**
   * 依赖声明快照（安装时写入安装清单，刷新仓库后由 `sync_dependency_snapshots`
   * 同步为线上声明）。
   *
   * 前端优先用在线仓库的 `relations`；仓库不在当前配置里（源下线、自定义源
   * 被删）时退回这份，否则 MOD 管理页的依赖表会凭空消失。
   */
  relations?: RepositoryRelation[]
}
export interface ModConfigDocument { guid: string; file_name: string; content: string; revision: string }
/** MOD 安装预览：本地文件 / 在线直链在安装前的解析结果。 */
export type ModPreviewRole = 'mod' | 'dependency'
export interface ModPreviewEntry {
  path: string
  role: ModPreviewRole
  name?: string | null
  guid?: string | null
  version?: string | null
  title?: string | null
  product_name?: string | null
  company?: string | null
  copyright?: string | null
  original_filename?: string | null
}
export interface ModPreview {
  preview_id: string
  /** single = 单 mod（恰好 1 个 DLL）；package = 包含 0 个或多个 DLL。 */
  kind: 'single' | 'package'
  package_name: string
  file_name: string
  total_dlls: number
  mod_count: number
  dependency_count: number
  entries: ModPreviewEntry[]
  /** 已安装的同身份 MOD；非空时必须先卸载再安装。 */
  conflicts: InstallConflict[]
  /** repository = 从在线仓库资产下载；manual = 本地文件或直链。 */
  origin: 'repository' | 'manual'
  /** 仓库来源本次 Release 的版本标签（如 v1.2.3）；手动来源为空串。 */
  release_tag: string
  /** 仓库来源本次 Release 的提交日志（Markdown）；手动来源为空串。 */
  release_body: string
}
/**
 * 安装来源：`local` = 本地文件的绝对路径，`url` = 在线直链。
 *
 * MOD 与地图都用这两个来源，只有后续处理不同（前者解析成预览、后者暂存成
 * 关卡集 ZIP），所以来源本身的形状只在这里定义一次。
 */
export interface InstallSource {
  kind: 'local' | 'url'
  /** 本地文件绝对路径，或在线直链 URL。 */
  value: string
}
export type ModSourceKind = InstallSource['kind']

/**
 * 拖进来的本地文件到底是什么包。
 *
 * 判据只有一条：**含 `info` 就是地图包**（地图包常带作者随包的修复插件，
 * 只按「有没有 DLL」判断会把它误判成 MOD）。来源由后端 `classify_drop_source`
 * 给出，前端两个域共用，不各自再判一次。
 */
export interface DropSourceKind {
  kind: 'map' | 'mod' | 'unknown'
  map_sets: number
  plugins: number
}
export type ConfigEditorTarget = { kind: 'mod'; mod: InstalledMod } | { kind: 'bepinex' }
export interface UpdateStatus { repository_id: string; local_version?: string; remote_version?: string; state: string }
export interface DashboardSummary { config: PublicConfig; game: GameInfo }
export interface Dashboard { config: PublicConfig; game: GameInfo; mods: InstalledMod[]; updates: UpdateStatus[] }
export interface OperationResult { ok: boolean; message: string }
/**
 * 依赖快照同步的结果（`sync_dependency_snapshots`）。
 *
 * 只回报「实际改写了多少条」：没变就不写盘，所以数字是 0 表示本地已经
 * 和线上声明一致，不是失败。
 */
export interface DependencySyncResult { mods_updated: number; maps_updated: number }
export interface BepInExInstallResult extends OperationResult { wine_override_applied: boolean; wine_override_error?: string; macos_overlay_repaired: boolean }
export interface ModRecycleEntry {
  id: string
  name: string
  /** 删除时仓库名的多语言快照；前端按当前语言现选，缺省回退 `name`。 */
  name_i18n?: Record<string, string>
  deleted_at: string
  file_count: number
  files: string[]
  game_path: string
  package_id?: string
  release_tag?: string
}
export interface UninstallLocalModResult extends OperationResult { requires_confirmation: boolean; requires_package_confirmation?: boolean; package_id?: string; package_mod_names?: string[] }
export interface RustLog { path: string; content: string }
/** 当前进程权限与可写性：应用不再启动即提权，凭这个判断是否需要提权。 */
export interface PermissionStatus {
  elevated: boolean
  game_writable: boolean
  data_writable: boolean
}
/** 仓库安装结果；预览模式下携带解析结果，供安装弹窗展示。 */
export interface ModRepositoryInstallOutcome extends OperationResult {
  preview?: ModPreview
}
export interface DownloadProgress {
  label: string
  downloaded: number
  total?: number
  speed_bytes: number
  finished: boolean
  phase?: 'connecting' | 'switching' | 'downloading' | 'merging' | 'finished'
  route?: 'direct' | 'proxy'
  source?: string
  attempt?: number
  attempt_total?: number
  parallel_sources?: number
  volumes?: {
    name: string
    state: 'pending' | 'downloading' | 'completed' | 'failed'
    retry: number
    progress: DownloadProgress
  }[]
}
export interface ImportPreview { valid: boolean; errors: string[]; additions: string[]; duplicates: string[]; conflicts: string[] }
export interface RepositorySourceImportResult {
  preview: ImportPreview | null
  access_error: string | null
  builtin_fallback_available: boolean
}
export interface AppUpdateInfo { version: string; current_version: string; body?: string; date?: string; download_size?: number; manifest_url: string; installer_url?: string; installer_signature: string; installer_name: string; installer_type?: 'exe' | 'msi' | 'app'; provider?: Provider; source_channel?: string }
export interface AppUpdateChannelStatus { id: string; provider: Provider; mode: string; status: string; error?: string }
export interface AppUpdateChannelEvent { region: string; channel: AppUpdateChannelStatus }
export interface AppUpdatePhaseEvent { region: string; phase: 'manifest' | 'download' | 'install' | 'skipped'; message?: string; update?: AppUpdateInfo }
export interface AppUpdateCheckResult { update?: AppUpdateInfo; channels: AppUpdateChannelStatus[]; all_failed: boolean; region: string }
export type DataProvider = 'github' | 'gitee' | 'builtin'
export interface SettingsPatch { ui_zoom?: number; hide_unidentified_mods?: boolean; recycle_deleted_mods?: boolean; game_path?: string; game_runtime?: GameRuntime; official_api_only?: boolean; timeout_seconds?: number; download_threads?: number; data_provider?: DataProvider; mod_metadata_concurrency?: number; map_info_concurrency?: number; map_compression_concurrency?: number; diychef_preview_concurrency?: number; tutorial_watched_steps?: string[] }
export interface TokenPatch { github_token: string; gitee_token: string; gitcode_token?: string; clear?: boolean }
export interface ModRepositoryPatch {
  id?: string
  name: string
  name_i18n?: Record<string, string>
  group: string
  group_i18n?: Record<string, string>
  provider: Provider
  username: string
  repository: string
  download_mode: ModDownloadMode
  branch: string
  file_path: string
  region?: string
  /** 支持的桌面平台；空 / 缺失表示全平台可用。 */
  platform?: RepositoryPlatform
  channel: Channel
  asset_rules: AssetRule[]
  install_target: string
  note: string
  note_i18n?: Record<string, string>
  plugin_guid: string
  local_product_name: string
  local_original_filename: string
  /** 这个 MOD 与其它插件的关系（依赖 / 排斥 / 兼容，按 GUID 声明）。 */
  relations?: RepositoryRelation[]
  /** 按 Release 版本覆盖关系表（见 ReleaseRelationOverride）。 */
  release_relations?: ReleaseRelationOverride[]
  version_regex: string
  max_release_count: number
  /** Legacy input only. Downloads use the resolved request plan. */
  download_threads?: number
  type?: 'mod' | 'map'
  network_plan?: NetworkPlanSelection
  github_token?: string
  gitee_token?: string
  official_api_only?: boolean
  enabled: boolean
}
/** 地图依赖的一个框架组件。 */
export interface RequiredFramework {
  guid: string
  /** 版本要求（pip 风格区间表达式）；空表示不限。 */
  version?: string
  /** 本地已装版本；未装时为空。 */
  installed_version?: string | null
}

/**
 * 一个地图仓库（地图下载页的展示单元）。
 *
 * 与 `ModRepository` 平行但不同模型：绑定靠 `level_set_name + level_set_name_zh + author`，
 * 不是 `plugin_guid`。共用的是卡片外观与下载流水线。
 */
export interface MapInfo {
  /** True only for maps generated from a release-tags source's API snapshot. */
  release_derived?: boolean
  id: string
  name: string
  name_i18n?: Record<string, string>
  group?: string
  group_i18n?: Record<string, string>
  provider: Provider
  username: string
  repository: string
  /** 绑定的本地关卡集名（info 里的 levelSetName）。 */
  level_set_name: string
  /**
   * 绑定的本地中文名（info 里的 levelSetNameZH），可选。
   *
   * 英文集名与作者都可能撞车（实测「胡闹糕中」与「胡闹达学」是同一位作者的
   * 两张图，都写 GAODA / GAODA），判「这张图装没装、装的是哪一版」必须一起看。
   * 老地图源不写这个字段，此时退回前两者比较。
   */
  level_set_name_zh?: string
  /** 绑定的本地作者（info 里的 author）。 */
  author: string
  note?: string
  note_i18n?: Record<string, string>
  enabled: boolean
  builtin: boolean
  /**
   * 所属地图源（`None` = 没有源归属）。
   *
   * 后端不持久化地图源树：源只是「源 → 分类 → 地图」折叠结构的第一层，
   * 所以归属与源名随每次加载源树写进条目本身，`get_maps` 原样透传。
   * 前端据此排源分组，不能在页面里重新猜归属。
   */
  source_id?: string | null
  /** 所属地图源的展示名（多语言表优先，单语言镜像兜底）。 */
  source_name?: string
  source_name_i18n?: Record<string, string>
  /** 缓存里最新 Release tag；没刷新过时为空。 */
  latest_tag?: string | null
  /**
   * 源文档钉死的「用户应该装的版本」（`[[maps]].latest_mapset_version`，即地图包版本）。
   *
   * 资产名里的版本号是作者手写的，`0.7` 按数字段比较反而小于 `0.69`；
   * 写了它就是「这就是最新版」，前端只比等值、不比大小（见 `mapUpdateState`）。
   * 旧块里的 `latest_version` 由后端 alias 兼容，前端只认新名字。
   */
  latest_mapset_version?: string | null
  /**
   * 最新 Release 里 **`info` 文件内的版本号**（`latest_info_version`）。
   *
   * 与上面的 `latest_mapset_version` 分工不同：那个是地图包（Release）的版本，由作者
   * 定义；这一份是包内 info 自己写的版本。两者平时相同，作者升了包版本却忘了
   * 改 info 里的版本号时才会不同 —— 此时拿本地 info 版本与它比，才能认出
   * 「装的已经是最新」（见 `levelSetStatus.ts::mapUpdateState`）。老地图源没有
   * 这个字段时判定自动退回包版本比较。
   */
  latest_info_version?: string | null
  /**
   * 缓存里最新的那一条 Release。
   *
   * 卡片复用时直接喂给版本号与下载量：`latest_tag` 只有字符串，
   * 拿不到 `assets`（下载量）和 `prerelease`（版本标记样式）。
   */
  latest_release?: ReleaseInfo | null
  /**
   * 缓存里的全部 Release（详情抽屉选版本用）。
   *
   * 卡片只需要最新一条，但「与 MOD 安装页对齐」的详情要允许用户装旧版本
   * 回滚；只给最新一条时那个版本下拉没有内容。
   */
  cached_releases: ReleaseInfo[]
  cached_release_count?: number
  cached_at?: number | null
  last_error?: string | null
  /**
   * 选哪个 Release 资产（地图只分发 ZIP）。
   *
   * 与 `ModRepository.asset_rules` 同一语义、同一套 `exact`/`glob`/`regex` 规则，
   * 由 `repositoryAssets.ts` 在前端镜像匹配 —— 详情页的资产下拉才能列出
   * 「实际会被安装的那个文件」，而不是把 Release 里所有附件都摊出来。
   */
  asset_rules?: AssetRule[]
  required_frameworks?: RequiredFramework[]
  /**
   * 仓库声明的全部关系（依赖 / 排斥 / 兼容，原始形态）。
   *
   * 与 `required_frameworks` 冗余但语义不同：那个只投影「依赖」给框架提示用，
   * 这里保留原始声明（含冲突/兼容、发布线、可选标记），依赖表要照原样展示。
   */
  relations?: RepositoryRelation[]
  /**
   * 按 Release 版本覆盖关系表（与 MOD 仓库同一语义、同一实现）。
   *
   * 同一张地图的不同版本可能依赖不同框架版本；命中哪条由 `versionMatches`
   * 判定，取**区间下限最高**的那条整体替换 `relations`（不合并）。
   */
  release_relations?: ReleaseRelationOverride[]
  /**
   * 这些依赖里当前没满足的那几条。
   *
   * 由**前端**用 Dashboard 里已经扫好的 MOD 判定（`resolveRelations` 与
   * MOD 侧共用一份实现）。地图页以前让后端再全量扫一遍 `BepInEx/plugins`
   * 只为拿 GUID → 版本，加载时间因此被浪费。字段保留兼容，但 `get_maps`
   * 不再填充。
   */
  unmet_frameworks?: RequiredFramework[]
}

/* ===== 地图发布（GitHub Release）===== */

/*
 * 这些字段名照抄后端 serde 的定义：Tauri 只把命令**顶层参数**从 camelCase
 * 转成 snake_case，嵌套结构体（`request`、`metadata`）原样反序列化，写成
 * camelCase 会被 serde 当成缺省值 —— 表现为「保存成功但元数据全空」。
 */

export interface PublishAuthState {
  authenticated: boolean
  login: string
  client_id: string
  /** GitHub App 安装入口：仓库可见范围由「安装时勾了哪些仓库」决定。 */
  install_url: string
  /**
   * 已经记下上次安装的 installation id。
   *
   * 为 true 时登录只重新授权（不再经过安装页），列仓库也按这个 id 直查 ——
   * `/user/installations` 是最终一致的，刚装完时它可能还返回空。
   */
  installed: boolean
  /** 构建时有没有注入 GitHub App 的 Client Secret（没有就换不到 Token）。 */
  secret_configured: boolean
}

/** 一次 GitHub App 用户授权的入口：浏览器地址与回调等待上限（秒）。 */
export interface PublishAuthorization {
  authorize_url: string
  expires_in: number
  /**
   * 这次打开的是哪个页面。
   *
   * `install` 是 App 安装页（第一次用，装 App 与勾仓库一步完成，回调带回
   * `installation_id`）；`authorize` 是纯 OAuth 授权（已经装过）。
   */
  mode: 'install' | 'authorize'
}

/**
 * 轮询结果。
 *
 * `pending` 只表示回调还没到；`failed` 的 reason 是后端完整 tag（含查询串），
 * 前端必须走 `translateBackendMessage` 翻译，不能直接显示。
 */
/**
 * 进入发布页时会话检查的结论（后端 `PublishSessionOutcome`）。
 *
 * `reauthorize`：令牌失效且刷新不了，本地令牌已清除；`unavailable`：网络或
 * GitHub 暂时不可用，本地令牌保留。两者的 `reason` 都是语言表 tag。
 */
export type PublishSessionOutcome =
  | { status: 'signed_out' }
  | { status: 'valid' }
  | { status: 'refreshed' }
  | { status: 'reauthorize'; reason: string }
  | { status: 'unavailable'; reason: string }

export interface PublishSessionCheck {
  outcome: PublishSessionOutcome
  auth: PublishAuthState
}

export type PublishAuthPoll =
  | { status: 'pending' }
  | { status: 'authorized'; login: string }
  | { status: 'expired' }
  | { status: 'denied' }
  | { status: 'failed'; reason: string }

export interface PublishRepository {
  owner: string
  name: string
  full_name: string
  private: boolean
  default_branch: string
  push: boolean
}

/** 发布页的仓库列表：安装数量与安装范围决定「新建仓库」能不能用。 */
export interface PublishRepositoryList {
  /** 授权到本 App 的安装数量；0 = 还没安装，必须先去安装页勾仓库。 */
  installations: number
  /** 有安装范围是「全部仓库」时，新建的仓库才自动在授权范围内。 */
  all_repositories: boolean
  repositories: PublishRepository[]
}

export type PublishProvider = 'github' | 'gitcode'

export interface PublishAsset {
  id: string
  name: string
  size: number
}

/** 本地 ZIP 摘要：上传取 info_version；原文件名的 mapset_version 仅供核对。set_count 必须为 1。 */
export interface PublishZipInspection {
  file_name: string
  file_size: number
  mapset_version: string
  info_version: string
  level_set_name: string
  level_set_name_zh: string
  author: string
  level_count: number
  file_count: number
  set_count: number
  requires: Record<string, string>
}

/**
 * 版本排序表的一项：一批资产在「谁是最新版」里的位置。
 *
 * 数组顺序就是从旧到新，表尾就是最新版。作者发修复版时常忘记改 info 版本
 * （`3.4.1-fix` 与 `3.4.1` 的 info 版本都是 `3.4.1`），只靠版本号比大小分不出
 * 新旧，所以顺序由作者拖动决定并写进块里。
 *
 * 表里**不存附件名**：附件名由 `asset` 规则渲染 `mapset_version` 得到。
 */
export interface AssetVersionEntry {
  /** 这一版的资产版本号，允许 `3.4.1-fix` 这种带后缀的文本。 */
  mapset_version: string
  /** 这一版的 info 版本号（ZIP 里 info 的版本）。 */
  info_version: string
}

/**
 * Release 说明里 `<!-- oc2-map -->` 块的投影。
 *
 * `asset` 是**带 `${version}` 占位符的模板**（后端生成时用），上传附件前要
 * 用这一版的版本号渲染成真实文件名，否则管理器按模板匹配不到 ZIP。
 */
export interface MapPublishMetadata {
  level_set_name: string
  level_set_name_zh: string
  author: string
  name_i18n: Record<string, string>
  group_i18n: Record<string, string>
  note_i18n: Record<string, string>
  asset: string
  latest_mapset_version: string
  latest_info_version: string
  asset_versions: AssetVersionEntry[]
  release_relations?: ReleaseRelationOverride[]
  requires: Record<string, string>
  compatible: Record<string, string>
  conflicts: Record<string, string>
}

export interface PublishRelease {
  id: string
  tag: string
  title: string
  draft: boolean
  prerelease: boolean
  created_at: string
  published_at: string | null
  body: string
  assets: PublishAsset[]
  /** null = 这个 Release 不是地图（说明里没有 oc2-map 块）。 */
  metadata: MapPublishMetadata | null
}

export interface PublishSaveRequest {
  provider: PublishProvider
  owner: string
  repository: string
  /** null = 新建（GitHub 按 tag 自动建），有值 = 修改这一版。 */
  release_id: string | null
  tag: string
  body: string
  /** 上传确认先提交完整地图声明；日常资料与版本保存也复用此字段。 */
  metadata?: MapPublishMetadata
}

export interface PublishUploadProgress {
  sent: number
  total: number
  phase: 'uploading' | 'confirming' | 'done'
}

export interface PublishUploadRequest {
  provider: PublishProvider
  owner: string
  repository: string
  release_id: string
  /** 本地 ZIP 的绝对路径；附件名由后端从 ZIP 版本和 Release 规则生成。 */
  path: string
}

export interface PublishDeleteReleaseRequest {
  provider: PublishProvider
  owner: string
  repository: string
  release_id: string
  tag: string
  /** 是否顺手删掉同名 git tag；tag 已不存在时后端视为成功。 */
  delete_tag: boolean
}

/** Startup diagnostics contain stable categories, never raw network errors. */
export interface StartupProbe {
  reachable: boolean
  failure_kind: 'busy' | 'cancelled' | 'timeout' | 'http' | 'network' | 'too_large' | 'invalid_content' | null
}

export interface SourceNode {
  id: string; parent_id?: string | null; label: string; label_i18n?: Record<string, string>;
  url?: string; status: 'pending' | 'loading' | 'ready' | 'error'; error?: string | null
}
export interface MapTreeProgress { nodes: SourceNode[]; maps: MapInfo[] }

export type { LevelPreview, LevelPreviewPicture, LevelPreviewRecipe, LevelPreviewDetail, LevelPreviewMenuIcon, LevelPreviewMenuTile } from './levelPreviewTypes'
