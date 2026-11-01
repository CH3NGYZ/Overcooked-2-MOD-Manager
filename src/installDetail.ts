/**
 * 「安装详情」卡片的展示模型 —— MOD 安装预览与地图 ZIP 导入共用一份。
 *
 * 为什么要有这一层：MOD 与地图看起来是两件事，但用户要确认的东西完全一样
 * （这个包会装进什么、哪些是插件、哪些只是依赖库、地图有几个关卡、旧版本
 * 会不会被覆盖）。以前地图用 Naive 的 `dialog.warning` 拼一段 VNode 正文，
 * MOD 用一整套可展开的详情卡片，同一批信息两种排版、两套文案。
 *
 * 现在把「数据 → 卡片」的规则收在这里，两个弹窗只负责渲染与确认：
 *   · MOD：`ModPreview`（后端 analyze 出的 DLL 清单）
 *   · 地图：暂存 ZIP 的识别结果（关卡集 + 随包插件）
 * 两边落到同一个 `InstallDetailModel`，差别只在数据来源与头部摘要。
 *
 * 条目的字段取值规则也只在这里定义一次：一个 DLL 展示九项元数据（名称 /
 * 版本 / 标题 / 产品名 / 作者 / 依赖 / GUID / 原始文件名 / 文件路径），
 * 后端读这些字段的实现同样只有一份（`mods::read_dll_details`）。
 */
import { t } from './i18n'
import { renderReadme } from './markdown'
import { formatVersion } from './modStatus'
import type { ModPreview, ModPreviewRole, ZipLevelSet } from './types'

/** 详情面板里的一格：左标签右值。 */
export interface InstallDetailField {
  label: string
  value: string
  /** 等宽字体：路径、GUID、版本号这类机器看得懂、人对得上的值。 */
  mono?: boolean
  /** 占满整行：长文本挤在半个格子里会被省略得看不出内容。 */
  wide?: boolean
}

/** 详情面板里的一张卡（一个 DLL 或一个地图集）。 */
export interface InstallDetailEntry {
  /** 稳定标识：展开状态按它记（DLL 是包内路径，地图集是目录名）。 */
  key: string
  title: string
  /** 标题下方那行灰字：DLL 是包内路径，地图集是「作者 · N 个关卡」。 */
  subtitle?: string
  /**
   * 收起态右侧版本号前面的角色标签：「依赖」/「随包插件」。
   *
   * 靠缩进表达从属关系时，右侧那列只能塞一个值（以前依赖项干脆把版本号
   * 换成「依赖」两个字）；改成标签后版本号还能照常显示，条目也可以平铺。
   */
  tag?: string
  /** 收起态右侧那列：版本号。 */
  meta?: string
  fields: InstallDetailField[]
  /** 卡片底部一行提示，例如「已有同名地图集，旧版本先进回收站」。 */
  note?: string
  /** 归属这张卡的下级条目：随包插件挂在它所属的地图集下面。 */
  children?: InstallDetailEntry[]
}

/** 一份待确认的安装：头部 + 条目 + 空态 + 底栏文案。 */
export interface InstallDetailModel {
  /** 弹窗标题（两种安装都叫「安装详情」）。 */
  title: string
  /** 头部：包名、类型标签、一行计数摘要。 */
  head: { name: string; kind: string; summary: string }
  entries: InstallDetailEntry[]
  /** 没有任何条目时的说明行；空数组表示这种安装不该出现空态。 */
  emptyHint: string[]
  /** 仓库来源的 Release 日志（纯 Markdown 已渲染成安全 HTML）；没有就整块不出现。 */
  releaseLog: { tag: string; html: string } | null
  confirmLabel: string
  cancelLabel: string
  /** 有没有可安装的内容；false 时「立即安装」禁用，只能取消。 */
  confirmable: boolean
}

/** 一个 DLL 的元数据来源：MOD 预览条目与随包插件字段完全一致。 */
interface DllDetailSource {
  path: string
  role: ModPreviewRole
  name?: string | null
  guid?: string | null
  version?: string | null
  title?: string | null
  product_name?: string | null
  company?: string | null
  original_filename?: string | null
}

/** 路径 → 文件名（两种分隔符都认；ZIP 条目一律是 `/`）。 */
export function fileNameOf(path: string): string {
  const file = path.replace(/\\/g, '/').split('/').pop() ?? path
  return file || path
}

/** 文件名 → 展示名：读不出任何元数据时用「去掉 .dll 的文件名」当标题。 */
export function dllDisplayName(path: string): string {
  return fileNameOf(path).replace(/\.dll$/i, '')
}

/** 卡片标题：先用插件名（用户在游戏里认识的那个名字），没有才退回文件名。 */
function dllTitle(entry: DllDetailSource): string {
  return entry.name?.trim() || dllDisplayName(entry.path)
}

/**
 * 收起态右侧那列：只有版本号。
 *
 * 「这是依赖、没有版本」这类信息改由 `tag` 表达，不再占用版本号的位置 ——
 * 同一个位置一会儿是版本、一会儿是说明，扫一眼分不清是哪种。
 */
function dllMeta(entry: DllDetailSource): string {
  return formatVersion(entry.version) || '—'
}

/** 一个 DLL 的九项元数据：顺序与前两格/后三格的分行方式都是固定版式。 */
function dllFields(entry: DllDetailSource): InstallDetailField[] {
  return [
    { label: t('modsLocal.name'), value: entry.name?.trim() || '—' },
    { label: t('modsOnline.versions'), value: formatVersion(entry.version) || '—', mono: true },
    { label: t('modsLocal.title'), value: entry.title?.trim() || '—' },
    { label: t('modsLocal.productName'), value: entry.product_name?.trim() || '—' },
    { label: t('modsLocal.author'), value: entry.company?.trim() || '—' },
    {
      label: t('tools.dependency'),
      value: entry.role === 'dependency' ? t('modsLocal.yes2') : t('modsLocal.no2'),
    },
    { label: 'GUID', value: entry.guid?.trim() || '—', mono: true, wide: true },
    { label: t('modsLocal.originalFilename'), value: entry.original_filename?.trim() || '—', mono: true, wide: true },
    { label: t('modsLocal.file2'), value: entry.path, mono: true, wide: true },
  ]
}

/** 一张 DLL 卡片：MOD 包里的一个 DLL，或地图包里随某个地图集落盘的插件。 */
function dllEntry(entry: DllDetailSource, tag?: string): InstallDetailEntry {
  return {
    key: entry.path,
    title: dllTitle(entry),
    subtitle: entry.path,
    tag,
    meta: dllMeta(entry),
    fields: dllFields(entry),
  }
}

/**
 * MOD 安装预览 → 详情模型。
 *
 * 仓库来源且本次 Release 写了日志时才带日志区块：本地文件与直链没有版本
 * 概念，RAW 仓库没有 Release，三种情况都让整块不出现（空方框只会占地方）。
 */
export function modPreviewDetail(preview: ModPreview): InstallDetailModel {
  const releaseBody = preview.release_body ?? ''
  const showReleaseLog = preview.origin === 'repository'
    && preview.release_tag.trim() !== ''
    && releaseBody.trim() !== ''
  return {
    title: t('tools.installDetails'),
    head: {
      name: preview.package_name,
      kind: preview.kind === 'single' ? t('tools.singleMod') : t('tools.modPackage'),
      summary: t('tools.dllsModsDependencies', {
        count: preview.total_dlls,
        mods: preview.mod_count,
        deps: preview.dependency_count,
      }),
    },
    // 依赖项打「依赖」标签：它们不是用户主动装的 MOD，只是别的 MOD 要用
    entries: preview.entries.map((entry) =>
      dllEntry(entry, entry.role === 'dependency' ? t('tools.dependency') : undefined)),
    emptyHint: [t('tools.thisArchiveContainsNoDllFiles'), t('tools.nothingInstallableHere')],
    releaseLog: showReleaseLog ? { tag: preview.release_tag, html: renderReadme(releaseBody) } : null,
    confirmLabel: t('tools.installNow'),
    cancelLabel: t('recycle.cancel'),
    confirmable: preview.total_dlls > 0,
  }
}

/**
 * 地图 ZIP（已暂存并识别）→ 详情模型。
 *
 * 一个地图集就是一个 MOD：版本、作者、关卡数来自 `info`；随包插件是它下面
 * 的另一个「MOD」（元数据从 DLL 里读，和 MOD 预览同一套字段）。插件挂在
 * `children` 上而不是平铺进 `entries`：渲染侧要能按「先全部地图集、再全部
 * 插件」重排，把它放到列表最底 —— 用户先确认要装哪几张图，再扫一眼会跟着
 * 落盘的 DLL。
 */
export function levelSetImportDetail(zipPath: string, sets: ZipLevelSet[]): InstallDetailModel {
  const pluginCount = sets.reduce((total, set) => total + (set.plugins?.length ?? 0), 0)
  return {
    title: t('tools.installDetails'),
    head: {
      name: fileNameOf(zipPath),
      kind: t('tools.mapPackage'),
      summary: t('tools.mapPackageSummary', { dlls: pluginCount, sets: sets.length }),
    },
    entries: sets.map(levelSetEntry),
    emptyHint: [],
    releaseLog: null,
    confirmLabel: t('tools.installNow'),
    cancelLabel: t('recycle.cancel'),
    confirmable: sets.length > 0,
  }
}

/** 一张地图集卡片：标题用中文名（作者与玩家平时叫的那个），没有才退回英文名/目录名。 */
function levelSetEntry(set: ZipLevelSet): InstallDetailEntry {
  const subtitle = [set.author?.trim(), t('levelSet.levelCount', { count: set.level_count })]
    .filter(Boolean)
    .join(' · ')
  return {
    key: set.dir_name,
    title: set.name_zh?.trim() || set.name?.trim() || set.dir_name,
    subtitle,
    meta: formatVersion(set.version) || '—',
    fields: [
      { label: t('levelSet.nameZh'), value: set.name_zh?.trim() || '—' },
      { label: t('levelSet.nameEn'), value: set.name?.trim() || '—' },
      { label: t('modsLocal.author'), value: set.author?.trim() || '—' },
      { label: t('levelSet.version'), value: formatVersion(set.version) || '—', mono: true },
      { label: t('levelSet.levelCountLabel'), value: String(set.level_count) },
      { label: t('levelSet.fileCountLabel'), value: String(set.file_count) },
      { label: t('levelSet.installFolder'), value: set.dir_name, mono: true, wide: true },
    ],
    note: set.exists ? t('levelSet.replaceHint') : undefined,
    // 随包插件一律打「随包插件」标签：不管它自己是 Mod 还是 Dependency，
    // 对用户来说都是「跟着这张图落盘、跟着这张图启停」的那一类
    children: (set.plugins ?? []).map((plugin) => dllEntry(plugin, t('levelSet.bundledPlugin'))),
  }
}
