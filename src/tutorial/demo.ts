import { ref } from 'vue'
import { i18n } from '../locale/index.ts'
import type {
  InstalledMod,
  MapInfo,
  ReleaseAsset,
  ReleaseInfo,
  Repository,
  RepositoryReadme,
  RepositorySource,
} from '../types'

/**
 * 教程演示数据：只存在于前端内存里，不落盘、不进 Dashboard 快照。
 *
 * ## 为什么需要
 *
 * 教程里有几组步骤是「指着一张卡片讲」：MOD 管理页的本地卡片、MOD 安装页的
 * 仓库卡片、地图下载页的地图卡片。新用户磁盘上什么都没有、首次启动又可能
 * 连不上任何仓库源，那些列表就是空的 —— 高亮框没东西可框，用户也看不到
 * 卡片长什么样。所以在教程运行期间，前端把这三处列表换成示例数据。
 *
 * ## 两个域，两条不同的合并规则
 *
 *   - **本地 MOD（`withDemoMods`）只在真实列表为空时补**：那是用户自己磁盘上
 *     的状态，有真卡就不该看见假卡。
 *   - **在线列表（`withDemoRepositories` / `withDemoSources` / `withDemoMaps`）
 *     在教程期间整体替换**：那里的内容取决于网络与用户配置，教程要讲的是固定的
 *     「源分组 → 卡片 → 详情 → 版本」路径，保留真实条目会让步骤指到随机一张卡
 *     （甚至一张都没有）。教程结束后 `App.vue` 的 `onTutorialSettled` 会发起
 *     真实刷新，页面当场回到真实数据。
 *
 * ## 边界（改动时不要越线）
 *
 *   · 只在**页面展示层**合并（`ModsLocalView` 的 `mods`、两个安装页的
 *     `repositories` / `sources` / `maps`），不写进 Dashboard 快照、配置或任何
 *     后端命令 —— 教程结束后页面 computed 当场回到真实数据，没有残留状态。
 *   · 示例仓库与示例地图的 owner 是一个不存在的账号，头像预取必须跳过
 *     （`isDemoOwner`）；示例条目的 README 由 `demoReadme` 直接给内置内容，
 *     不去请求后端（那会按一个不存在的 id 打网络）。
 *   · 示例名字都带「示例 / Sample」字样（`tutorial.demo.*`），用户看得出
 *     这不是自己装的东西。
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
let demoRepositories: Repository[] = []
let demoSources: RepositorySource[] = []
let demoMaps: MapInfo[] = []

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
 * 两张示例 MOD。
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
      product_name: 'TutorialDemoModA',
      original_filename: 'TutorialDemoModA.dll',
      guid: 'oc2.tutorial.demo.mod-a',
      company: 'OC2 MOD Manager',
      copyright: t('tutorial.demo.author'),
      dll_file: 'TutorialDemoModA.dll',
      enabled: true,
      version: '1.2.0',
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
      product_name: 'TutorialDemoModB',
      original_filename: 'TutorialDemoModB.dll',
      guid: 'oc2.tutorial.demo.mod-b',
      company: 'OC2 MOD Manager',
      copyright: t('tutorial.demo.author'),
      dll_file: 'TutorialDemoModB.dll',
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
  ]
}

interface DemoVersion {
  tag: string
  published: string
  downloads: number
}

/** 一条示例 Release：卡片要的版本号、下载量与资产下拉都由它提供。 */
function demoRelease(version: DemoVersion): ReleaseInfo {
  const asset: ReleaseAsset = {
    name: `TutorialSample-${version.tag}.zip`,
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
  versions: DemoVersion[]
}

function demoRepository(spec: DemoRepositorySpec): Repository {
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
    local_product_name: spec.title,
    local_original_filename: 'TutorialSample.dll',
    version_regex: '',
    max_release_count: 10,
    download_threads: 4,
    enabled: true,
    builtin: true,
    cached_releases: spec.versions.map(demoRelease),
  }
}

interface DemoMapSpec {
  id: string
  title: string
  levelSetName: string
  versions: DemoVersion[]
}

function demoMap(spec: DemoMapSpec): MapInfo {
  const releases = spec.versions.map(demoRelease)
  const latest = releases[0] ?? null
  return {
    id: spec.id,
    name: spec.title,
    group: t('tutorial.demo.group'),
    provider: 'github',
    username: DEMO_OWNER,
    repository: DEMO_REPOSITORY,
    // 身份字段写示例值：本地永远匹配不到，卡片状态因此稳定停在「未安装」。
    level_set_name: spec.levelSetName,
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

function buildDemoRepositories(): Repository[] {
  return [
    demoRepository({
      id: `${DEMO_ID_PREFIX}repo-a`,
      title: t('tutorial.demo.modA'),
      guid: 'oc2.tutorial.demo.repo-a',
      versions: [
        { tag: '1.2.0', published: '2026-05-18T09:00:00Z', downloads: 4120 },
        { tag: '1.1.0', published: '2026-03-02T09:00:00Z', downloads: 2870 },
      ],
    }),
    demoRepository({
      id: `${DEMO_ID_PREFIX}repo-b`,
      title: t('tutorial.demo.modB'),
      guid: 'oc2.tutorial.demo.repo-b',
      versions: [{ tag: '1.0.4', published: '2026-04-11T09:00:00Z', downloads: 1560 }],
    }),
  ]
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
  demoActive.value = true
}

/** 教程结束时调用（`engine.finishTutorial`），必须在 settled 回调之前，让启动刷新面对真实数据。 */
export function stopTutorialDemo(): void {
  demoActive.value = false
  demoMods = []
  demoSources = []
  demoRepositories = []
  demoMaps = []
}

/**
 * 本地 MOD 列表的展示合并：非空就用真实数据，空的才补示例。
 *
 * 未在演示态、或真实列表非空时返回**原引用**（不复制、不新建数组）：
 * 调用方多半是 computed，同一个引用能让下游的 `watch`/`computed` 原地不动。
 */
export function withDemoMods(real: readonly InstalledMod[]): InstalledMod[] {
  if (!demoActive.value || real.length > 0) return real as InstalledMod[]
  return demoMods
}

/**
 * MOD 安装页的源列表：教程期间整体替换成示例源。
 *
 * 必须和 `withDemoRepositories` 一起用 —— 仓库可用性判定会查「所属源是否
 * 存在」，只换仓库不换源的话示例卡片会被 `repositoryIsAvailable` 全部滤掉，
 * 页面看起来比教程之前还空。
 */
export function withDemoSources(real: readonly RepositorySource[]): RepositorySource[] {
  if (!demoActive.value) return real as RepositorySource[]
  return demoSources
}

/** MOD 安装页的仓库列表：教程期间整体替换成示例仓库（理由见文件头）。 */
export function withDemoRepositories(real: readonly Repository[]): Repository[] {
  if (!demoActive.value) return real as Repository[]
  return demoRepositories
}

/** 地图下载页的地图列表：教程期间整体替换成示例地图（理由见文件头）。 */
export function withDemoMaps(real: readonly MapInfo[]): MapInfo[] {
  if (!demoActive.value) return real as MapInfo[]
  return demoMaps
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
