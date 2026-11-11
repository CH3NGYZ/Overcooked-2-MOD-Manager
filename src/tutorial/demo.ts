import { ref } from 'vue'
import { i18n } from '../locale/index.ts'
import type {
  InstalledMod,
  GameInfo,
  LevelSetInfo,
  LevelSetCompressionProgress,
  LevelPreview,
  LevelPreviewDetail,
  MapInfo,
  ReleaseAsset,
  ReleaseInfo,
  ModRepository,
  PublicConfig,
  RepositoryReadme,
  RepositorySource,
  SourceNode,
} from '../types'

/**
 * 教程演示数据：只存在于前端内存里，不落盘、不进 Dashboard 快照。
 *
 * ## 为什么需要
 *
 * 教程里有几组步骤是「指着一张卡片讲」：MOD 管理页的本地卡片、MOD 安装页的
 * 仓库卡片、地图下载页的地图卡片。新用户磁盘上什么都没有、首次启动又可能
 * 连不上任何仓库源，那些列表就是空的 —— 高亮框没东西可框，用户也看不到
 * 卡片长什么样。所以在教程运行期间，前端把相关列表与摘要换成示例数据。
 *
 * 本地 MOD、地图、在线条目、源树与概览在教程期间全部使用同一批示例。
 * 真实列表即使非空也整体替换：教程要讲的是固定的
 *     「源分组 → 卡片 → 详情 → 版本」路径，保留真实条目会让步骤指到随机一张卡
 *     （甚至一张都没有）。教程结束后 `App.vue` 的 `onTutorialSettled` 会发起
 *     真实刷新，页面当场回到真实数据；真实请求迟到也只改变真实缓存。
 *
 * ## 边界（改动时不要越线）
 *
 *   · 只在**页面展示层**合并（`ModsLocalView` 的 `mods`、两个安装页的
 *     `repositories` / `sources` / `maps` / `sets`），不写进 Dashboard 快照、配置或任何
 *     后端命令 —— 教程结束后页面 computed 当场回到真实数据，没有残留状态。
 *   · 示例仓库与示例地图的 owner 是一个不存在的账号，头像预取必须跳过
 *     （`isDemoOwner`）；示例条目的 README 由 `demoReadme` 直接给内置内容，
 *     不去请求后端（那会按一个不存在的 id 打网络）。
 *   · 标题取社区常见的 MOD 名（BepInEx 配置管理器 / 街机 MOD），让教程里
 *     那一屏就是用户真正会看到的样子；「这是演示数据」由仓库说明、README
 *     与示例 GUID 前缀承担。
 *   · 教程浮层接管全部点击（点哪儿都是「下一步」），示例卡片不可能被点到，
 *     因此不会触发开关、删除、安装这些写操作；示例资产地址写成 `.invalid`，
 *     万一谁真的发起了下载，也会当场失败而不是打到别的站点。
 */

/** 教程正在运行：只有它为真时，合并函数才会考虑示例数据。 */
export const demoActive = ref(false)

/** 示例数据所有 id 的前缀：判断「这是不是教程示例」只认它。 */
export const DEMO_ID_PREFIX = 'oc2.tutorial.demo.'
/** MOD 安装页第一层的示例源；示例仓库都挂在它下面（否则卡片不显示）。 */
export const DEMO_SOURCE_ID = `${DEMO_ID_PREFIX}source`
/** 地图下载页的示例源：地图源不落盘，源名跟着条目自己的 `source_name` 走。 */
export const DEMO_MAP_SOURCE_ID = `${DEMO_ID_PREFIX}map-source`
/** 示例条目共用的 owner：不存在这个账号，头像预取必须跳过。 */
const DEMO_OWNER = 'OC2TutorialSample'
/** 示例条目共用的仓库名（卡片不显示它，但模型要求非空）。 */
const DEMO_REPOSITORY = 'OC2-Tutorial-Sample'
/** 假的资产地址：`.invalid` 是保留域名，误用会当场失败而不是打到真实站点。 */
const DEMO_DOWNLOAD_URL = 'https://example.invalid/tutorial-sample.zip'

/** 当前这批示例数据。`startTutorialDemo()` 重建，`stopTutorialDemo()` 清空。 */
let demoMods: InstalledMod[] = []
let demoRepositories: ModRepository[] = []
let demoSources: RepositorySource[] = []
let demoMaps: MapInfo[] = []
let demoLevelSets: LevelSetInfo[] = []
let demoMapSourceNodes: SourceNode[] = []
let demoConfig: PublicConfig | null = null
const DEMO_DIYLEVEL_DLL = 'oc2.tutorial.demo.DIYLevel.dll'
const demoGame: GameInfo = {
  path: 'C:\\OC2-Tutorial',
  game_exists: true,
  bepinex: { installed: true, has_residue: false, has_config: true, version: '5.4.23.2', backups: [] },
  kitchen_running: false,
  winhttp_state: 'modenabled',
}
const demoCompression: LevelSetCompressionProgress = {
  running: false, sets_done: 0, sets_total: 0, set_name: '',
  files_done: 0, files_total: 0, done: 0, total: 0,
  converted: 0, skipped: 0, failed: 0, percent: 0, current: '',
}

function t(key: string): string {
  return i18n.global.t(key)
}

/** 这个 id 是不是示例条目（README、安装等按 id 分支的地方都用它）。 */
export function isDemoId(id: string | null | undefined): boolean {
  return !!id && id.startsWith(DEMO_ID_PREFIX)
}

/**
 * 这个 owner 是不是示例账号。
 *
 * 示例仓库/地图的 owner 并不存在，头像预取必须跳过：否则每次教程都会向头像
 * 接口打一次必然失败的请求，日志里全是噪音，离线时还会多等一次超时。
 */
export function isDemoOwner(owner: string | null | undefined): boolean {
  return owner?.trim().toLowerCase() === DEMO_OWNER.toLowerCase()
}

/**
 * 三张示例 MOD，与在线仓库和 DIYLevel 概览共用同一组身份。
 *
 * 故意不设 `plugin_name`：卡片标题的取值顺序是
 * `plugin_name → title → product_name → name`（`ModsLocalView` 的 `modTitle`），
 * 设了 `plugin_name` 就绕过了语言切换，所以本地化名字写在 `title`/`name` 上。
 */
function buildDemoMods(): InstalledMod[] {
  return [
    {
      name: t('tutorial.demo.modA'),
      title: t('tutorial.demo.modA'),
      product_name: 'BepInEx.ConfigurationManager',
      original_filename: 'BepInEx.ConfigurationManager.dll',
      guid: 'oc2.tutorial.demo.mod-a',
      repository_id: `${DEMO_ID_PREFIX}repo-a`,
      company: 'BepInEx',
      copyright: t('tutorial.demo.author'),
      dll_file: 'BepInEx.ConfigurationManager.dll',
      enabled: true,
      version: '17.1',
      version_source: 'metadata',
      update_available: false,
      duplicate_guid: false,
      manual_install: true,
      is_dependency: false,
      has_config: true,
      has_logs: false,
      status: 'latest',
    },
    {
      name: t('tutorial.demo.modB'),
      title: t('tutorial.demo.modB'),
      product_name: 'ArcadeMod',
      original_filename: 'ArcadeMod.dll',
      guid: 'oc2.tutorial.demo.mod-b',
      repository_id: `${DEMO_ID_PREFIX}repo-b`,
      // 厂商照社区里街机 MOD 的作者写，卡片详情的「公司」一栏才像真的；
      // 「这是演示数据」由 GUID 前缀与仓库说明承担，不靠厂商字段。
      company: 'CH3NGYZ',
      copyright: t('tutorial.demo.author'),
      dll_file: 'ArcadeMod.dll',
      enabled: true,
      version: '1.0.4',
      version_source: 'metadata',
      update_available: false,
      duplicate_guid: false,
      manual_install: true,
      is_dependency: false,
      has_config: false,
      has_logs: false,
      status: 'latest',
    },
    {
      name: t('framework.diyLevel.title'),
      title: t('framework.diyLevel.title'),
      guid: `${DEMO_ID_PREFIX}diylevel`,
      repository_id: `${DEMO_ID_PREFIX}repo-diylevel`,
      product_name: 'OC2DIYLevel',
      original_filename: 'OC2DIYLevel.dll',
      dll_file: DEMO_DIYLEVEL_DLL,
      company: t('tutorial.demo.author'),
      enabled: true,
      version: '2.1.0',
      version_source: 'metadata',
      update_available: false,
      duplicate_guid: false,
      has_config: false,
      status: 'latest',
    },
  ]
}

interface DemoVersion {
  tag: string
  published: string
  downloads: number
}

/**
 * 一条示例 Release：卡片要的版本号、下载量与资产下拉都由它提供。
 *
 * `assetBase` 必填：资产名会显示在版本下拉与资产列表里，写死示例名会跟卡片
 * 标题对不上。注意别直接写 `versions.map(demoRelease)` —— 数组 `map` 会把
 * 下标当第二个参数传进来。
 */
function demoRelease(version: DemoVersion, assetBase: string): ReleaseInfo {
  const asset: ReleaseAsset = {
    name: `${assetBase}-${version.tag}.zip`,
    size: 2_411_520,
    download_count: version.downloads,
    download_url: DEMO_DOWNLOAD_URL,
  }
  return {
    tag_name: version.tag,
    version: version.tag.replace(/^v/, ''),
    name: version.tag,
    body: t('tutorial.demo.releaseBody'),
    published_at: version.published,
    prerelease: false,
    draft: false,
    assets: [asset],
  }
}

interface DemoRepositorySpec {
  id: string
  title: string
  guid: string
  /** 本地已装匹配用的产品名 / DLL 名：与本地示例 MOD 那张卡保持同一组名字。 */
  productName: string
  dllName: string
  /** Release 资产名前缀（`<assetBase>-<tag>.zip`）。 */
  assetBase: string
  versions: DemoVersion[]
}

function demoRepository(spec: DemoRepositorySpec): ModRepository {
  return {
    id: spec.id,
    name: spec.title,
    // 分类名与仓库名都按当前界面语言直接写进字段：示例数据每次开始教程时重建，
    // 用户中途改语言后再播一遍就会用新语言（不需要额外的 i18n 表）。
    group: t('tutorial.demo.group'),
    provider: 'github',
    username: DEMO_OWNER,
    repository: DEMO_REPOSITORY,
    download_mode: 'release',
    branch: 'main',
    file_path: '',
    channel: 'stable',
    asset_rules: [{ type: 'glob', value: '*.zip' }],
    install_target: 'BepInEx/plugins',
    note: t('tutorial.demo.repoNote'),
    source_id: DEMO_SOURCE_ID,
    plugin_guid: spec.guid,
    local_product_name: spec.productName,
    local_original_filename: spec.dllName,
    version_regex: '',
    max_release_count: 10,
    download_threads: 4,
    enabled: true,
    builtin: true,
    cached_releases: spec.versions.map((version) => demoRelease(version, spec.assetBase)),
  }
}

interface DemoMapSpec {
  id: string
  title: string
  levelSetName: string
  versions: DemoVersion[]
}

function demoMap(spec: DemoMapSpec): MapInfo {
  const releases = spec.versions.map((version) => demoRelease(version, 'TutorialSampleMap'))
  const latest = releases[0] ?? null
  return {
    id: spec.id,
    name: spec.title,
    group: t('tutorial.demo.group'),
    provider: 'github',
    username: DEMO_OWNER,
    repository: DEMO_REPOSITORY,
    // 本地地图复用同一身份，两个地图页面与概览得到同一个版本结论。
    level_set_name: spec.levelSetName,
    level_set_name_zh: spec.title,
    author: t('tutorial.demo.author'),
    note: t('tutorial.demo.repoNote'),
    enabled: true,
    builtin: true,
    source_id: DEMO_MAP_SOURCE_ID,
    source_name: t('tutorial.demo.sourceName'),
    latest_tag: latest?.tag_name ?? null,
    latest_mapset_version: latest?.version ?? null,
    latest_info_version: latest?.version ?? null,
    latest_release: latest,
    cached_releases: releases,
    cached_release_count: releases.length,
    cached_at: null,
    asset_rules: [{ type: 'glob', value: '*.zip' }],
  }
}

function buildDemoSources(): RepositorySource[] {
  return [
    {
      id: DEMO_SOURCE_ID,
      nickname: t('tutorial.demo.sourceName'),
      url: '',
      builtin: true,
    },
  ]
}

function buildDemoRepositories(): ModRepository[] {
  return [
    demoRepository({
      id: `${DEMO_ID_PREFIX}repo-a`,
      title: t('tutorial.demo.modA'),
      guid: 'oc2.tutorial.demo.mod-a',
      productName: 'BepInEx.ConfigurationManager',
      dllName: 'BepInEx.ConfigurationManager.dll',
      assetBase: 'BepInEx.ConfigurationManager',
      versions: [
        { tag: '17.1', published: '2026-05-18T09:00:00Z', downloads: 4120 },
        { tag: '17.0', published: '2026-03-02T09:00:00Z', downloads: 2870 },
      ],
    }),
    demoRepository({
      id: `${DEMO_ID_PREFIX}repo-b`,
      title: t('tutorial.demo.modB'),
      guid: 'oc2.tutorial.demo.mod-b',
      productName: 'ArcadeMod',
      dllName: 'ArcadeMod.dll',
      assetBase: 'ArcadeMod',
      versions: [{ tag: '1.0.4', published: '2026-04-11T09:00:00Z', downloads: 1560 }],
    }),
    demoRepository({
      id: `${DEMO_ID_PREFIX}repo-diylevel`,
      title: t('framework.diyLevel.title'),
      guid: `${DEMO_ID_PREFIX}diylevel`,
      productName: 'OC2DIYLevel',
      dllName: 'OC2DIYLevel.dll',
      assetBase: 'OC2DIYLevel',
      versions: [{ tag: '2.1.0', published: '2026-06-02T09:00:00Z', downloads: 2400 }],
    }),
  ]
}

function buildDemoLevelSets(): LevelSetInfo[] {
  return demoMaps.map((map, index) => ({
    dir_name: `${DEMO_ID_PREFIX}level-set-${index + 1}`,
    path: `C:\\OC2-Tutorial\\BepInEx\\plugins\\OC2DIYLevel\\levels\\${map.level_set_name}`,
    enabled: true,
    name: map.level_set_name,
    name_zh: map.level_set_name_zh,
    author: map.author,
    version: map.latest_info_version ?? '',
    installed_mapset_version: map.latest_mapset_version ?? '',
    uid: `${DEMO_ID_PREFIX}uid-${index + 1}`,
    base_uid: `${DEMO_ID_PREFIX}base-${index + 1}`,
    level_count: 1,
    map_id: map.id,
    map_group: map.group,
    levels: [{ name: map.name, name_zh: map.name, scene_name: `${DEMO_ID_PREFIX}scene-${index + 1}`, min_order_count: 2, max_order_count: 5 }],
  }))
}

function buildDemoConfig(): PublicConfig {
  const macos = typeof navigator !== 'undefined' && /mac/i.test(navigator.userAgent)
  return {
    schema_version: 1, platform: macos ? 'macos' : 'windows', region: 'global', ui_zoom: 1.25,
    hide_unidentified_mods: false, recycle_deleted_mods: true,
    mod_metadata_concurrency: 0, map_info_concurrency: 0, map_compression_concurrency: 0, diychef_preview_concurrency: 0,
    game: { path: demoGame.path!, runtime: 'native' },
    network: { timeout_seconds: 30, download_threads: 4, max_retry_count: 3 },
    launch: { ask_user_to_start_from_steam: true, auto_restore_winhttp: true },
    official_api_only: false, github_token_set: false, gitee_token_set: false, gitcode_token_set: false,
    repositories: demoRepositories, repository_sources: demoSources,
    github_proxies: { release_proxies: [], raw_proxies: [], github_api_proxies: [], gitee_api_proxies: [], gitcode_api_proxies: [], avatar_proxies: [] },
    data_provider: 'builtin',
  }
}

function buildDemoMaps(): MapInfo[] {
  return [
    demoMap({
      id: `${DEMO_ID_PREFIX}map-a`,
      title: t('tutorial.demo.mapA'),
      levelSetName: 'TutorialSampleMapA',
      versions: [
        { tag: '1.4.0', published: '2026-06-02T09:00:00Z', downloads: 980 },
        { tag: '1.3.0', published: '2026-04-20T09:00:00Z', downloads: 640 },
      ],
    }),
    demoMap({
      id: `${DEMO_ID_PREFIX}map-b`,
      title: t('tutorial.demo.mapB'),
      levelSetName: 'TutorialSampleMapB',
      versions: [{ tag: '1.0.2', published: '2026-05-09T09:00:00Z', downloads: 410 }],
    }),
  ]
}

/**
 * 教程开始时调用（`engine.startTutorial`）。
 *
 * 每次都重建：界面语言可能在两次教程之间被改过，示例名字要跟着走。
 * 已经在演示态时直接返回，重复开始不会叠两批数据。
 */
export function startTutorialDemo(): void {
  if (demoActive.value) return
  demoMods = buildDemoMods()
  demoSources = buildDemoSources()
  demoRepositories = buildDemoRepositories()
  demoMaps = buildDemoMaps()
  demoLevelSets = buildDemoLevelSets()
  demoMapSourceNodes = [{ id: DEMO_MAP_SOURCE_ID, label: t('tutorial.demo.sourceName'), status: 'ready' }]
  demoConfig = buildDemoConfig()
  demoActive.value = true
}

/** 教程结束时调用（`engine.finishTutorial`），必须在 settled 回调之前，让启动刷新面对真实数据。 */
export function stopTutorialDemo(): void {
  demoActive.value = false
  demoMods = []
  demoSources = []
  demoRepositories = []
  demoMaps = []
  demoLevelSets = []
  demoMapSourceNodes = []
  demoConfig = null
}

/**
 * 本地 MOD 列表与在线列表一样整体替换。未在演示态时返回**原引用**：
 * 调用方多半是 computed，同一个引用能让下游的 `watch`/`computed` 原地不动。
 */
export function withDemoMods(real: readonly InstalledMod[]): InstalledMod[] {
  if (!demoActive.value) return real as InstalledMod[]
  return demoMods
}

/**
 * MOD 安装页的源列表：教程期间整体替换成示例源。
 *
 * 必须和 `withDemoRepositories` 一起用 —— 仓库可用性判定会查「所属源是否
 * 存在」，只换仓库不换源的话示例卡片会被 `modRepositoryIsAvailable` 全部滤掉，
 * 页面看起来比教程之前还空。
 */
export function withDemoSources(real: readonly RepositorySource[]): RepositorySource[] {
  if (!demoActive.value) return real as RepositorySource[]
  return demoSources
}

/** MOD 安装页的仓库列表：教程期间整体替换成示例仓库（理由见文件头）。 */
export function withDemoRepositories(real: readonly ModRepository[]): ModRepository[] {
  if (!demoActive.value) return real as ModRepository[]
  return demoRepositories
}

/** 地图下载页的地图列表：教程期间整体替换成示例地图（理由见文件头）。 */
export function withDemoMaps(real: readonly MapInfo[]): MapInfo[] {
  if (!demoActive.value) return real as MapInfo[]
  return demoMaps
}

/** 本地地图、源树及游戏摘要都只投影展示，不写入对应共享 store。 */
export function withDemoLevelSets(real: readonly LevelSetInfo[]): LevelSetInfo[] {
  return demoActive.value ? demoLevelSets : real as LevelSetInfo[]
}

export function withDemoMapSourceNodes(real: readonly SourceNode[]): SourceNode[] {
  return demoActive.value ? demoMapSourceNodes : real as SourceNode[]
}

export function withDemoGame(real: GameInfo | null): GameInfo | null {
  return demoActive.value ? demoGame : real
}

export function withDemoLevelCompression(real: LevelSetCompressionProgress | null): LevelSetCompressionProgress | null {
  return demoActive.value ? demoCompression : real
}

export function withDemoDiyLevelTarget(real: string | null): string | null {
  return demoActive.value ? DEMO_DIYLEVEL_DLL : real
}

export function withDemoConfig(real: PublicConfig | null): PublicConfig | null {
  return demoActive.value ? demoConfig : real
}

/** 示例目录永久识别；退出时尚在离场动画中的预览也不能去读真实路径。 */
export function demoLevelPreview(set: LevelSetInfo): LevelPreview[] | null {
  if (!isDemoId(set.dir_name)) return null
  return (set.levels ?? []).map(level => ({
    ...level, name_zh: level.name_zh ?? '', dependencies: [], scene_exists: true,
    recipe_count: 0, active_recipe_count: 0,
  }))
}

export function demoLevelPreviewDetail(set: LevelSetInfo): LevelPreviewDetail | null {
  return isDemoId(set.dir_name)
    ? { picture: { data_url: null, width: 0, height: 0, state: 'missing' }, recipes: [] }
    : null
}

/**
 * 示例条目的 README。
 *
 * 示例仓库没有真实 id，`repositoryReadme` 命令按不存在的仓库去请求必然失败，
 * 抽屉里就会出现一块红色报错。这里直接给内置说明；返回 `null` 表示「不是
 * 示例条目，走正常请求」。
 */
export function demoReadme(id: string | null | undefined): RepositoryReadme | null {
  if (!demoActive.value || !isDemoId(id)) return null
  return { content: t('tutorial.demo.repoReadme'), exists: true, url: '' }
}
