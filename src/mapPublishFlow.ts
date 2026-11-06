/**
 * 发布页三个步骤的纯逻辑。
 *
 * 一张地图集的三件事**各自保存、互不捆绑**：
 *
 * 1. `release`：GitHub Release 的固定 tag 与发布说明。新地图在这里建 Release；
 *    已有地图只改这些字段，后端保留原有的 oc2-map 块。
 * 2. `asset`：上传这一版的 ZIP 附件。只写附件，不碰块。
 * 3. `metadata`：写 oc2-map 块（名称、多语言、版本表、关系表）。**只有登记进版本表
 *    的附件才对玩家可见**（消费端 `synthesize_releases` 在有表时只认表内附件），所以
 *    「上传」与「发布给玩家」被明确拆成两步，中间失败不会露出半成品。
 *
 * 纯模块（不依赖 Vue / i18n / 组件）：`tests/map-publish-flow.test.mjs` 直接
 * import；页面与 composable 只负责发请求与显示。
 */

// 带 `.ts` 扩展名：Node 的 `--test` 直接跑源码（类型剥离不做补全），Vite 与
// `allowImportingTsExtensions` 都接受这种写法。
import type { PublishZipInspection } from './types.ts'
import { isVersionText, levelSetSlug } from './mapPublishAsset.ts'

export type PublishStepId = 'release' | 'asset' | 'metadata'

/** 导航顺序：没有 Release 就没有地方挂附件，块也要写在已有 Release 上。 */
export const PUBLISH_STEP_ORDER: readonly PublishStepId[] = ['release', 'asset', 'metadata']

/** 上传完成后必须保存地图资料，不能回退去改 Release 或再次上传。 */
export function stepAvailable(step: PublishStepId, releaseExists: boolean, uploadPending = false): boolean {
  if (uploadPending) return releaseExists && step === 'metadata'
  return step === 'release' || releaseExists
}

/** 某一步保存前还缺什么；空数组才允许点这一步的保存按钮。 */
export type StepIssue = 'tag' | 'release' | 'zip' | 'version' | 'levelSetName' | 'versionRows'

export interface StepInput {
  /** 已确定的 tag（新地图由作者填写，已有地图用现有 tag）。 */
  tag: string
  releaseExists: boolean
  /** 按资产规则渲染出的附件名；空 = 还没选到能算出名字的 ZIP。 */
  assetName: string
  /** 这一版的版本号文本（允许 `3.4.1-fix` 这种后缀）。 */
  versionText: string
  /** oc2-map 块里的关卡集英文名，写块时必须非空。 */
  levelSetName: string
  /** 版本表里的行数（只算对得上附件的行）。 */
  versionCount: number
}

/**
 * 每一步各自的前置条件。
 *
 * 写块要求版本表非空：块是消费端识别地图的依据，零版本的块会让一张下载不到的
 * 图出现在管理器里。
 */
export function stepIssues(step: PublishStepId, input: StepInput): StepIssue[] {
  const issues: StepIssue[] = []
  switch (step) {
    case 'release':
      if (!isMapPublishTag(input.tag.trim())) issues.push('tag')
      break
    case 'asset':
      if (!input.releaseExists) issues.push('release')
      if (!input.assetName.trim()) issues.push('zip')
      if (!isVersionText(input.versionText.trim())) issues.push('version')
      break
    case 'metadata':
      if (!input.releaseExists) issues.push('release')
      if (!input.levelSetName.trim()) issues.push('levelSetName')
      if (input.versionCount === 0) issues.push('versionRows')
      break
  }
  return issues
}

/**
 * 已上传但还没登记进远端版本表的版本（保持 `rows` 的顺序）。
 *
 * 这些附件在 GitHub 上已经存在，但玩家看不到；界面据此提示作者去第三步保存。
 */
export function unregisteredVersions(
  rows: readonly string[],
  registered: readonly string[],
): string[] {
  const saved = new Set(registered.map((version) => version.trim()).filter(Boolean))
  return rows.map((version) => version.trim()).filter((version) => version && !saved.has(version))
}

/**
 * 地图集名 → 默认 tag。
 *
 * tag 是这张地图在仓库里的身份（一个 tag 一张图），不是版本号；作者可以随意
 * 改，这里只给一个符合 git ref 规则的起点：小写字母数字与 `-`，去掉首尾横线。
 * 名字里没有 ASCII 字母数字时返回空串，由作者自己填（中文 tag 也是合法的）。
 */
export function defaultMapTag(levelSetName: string): string {
  return levelSetSlug(levelSetName).slice(0, 100).replace(/^[-.]+|-+$/g, '')
}

/** Tag 输入只接受 ASCII 字母与数字。 */
export function isMapPublishTag(value: string): boolean {
  return /^[A-Za-z0-9]+$/.test(value)
}

/** 比较 ZIP 身份的三个字段，空值也参与比较。 */
export function zipIdentityMismatch(
  metadata: { level_set_name: string; level_set_name_zh: string; author: string },
  inspection: { level_set_name: string; level_set_name_zh: string; author: string },
): boolean {
  return metadata.level_set_name !== inspection.level_set_name ||
    metadata.level_set_name_zh !== inspection.level_set_name_zh ||
    metadata.author !== inspection.author
}

export type PublishZipIssue = 'multiple' | 'version' | 'identity'

/** 展示识别结果后再判可上传性；失败的结果仍用于只读表格，不能写入资料草稿。 */
export function publishZipIssue(
  inspection: PublishZipInspection,
  identity: Pick<PublishZipInspection, 'level_set_name' | 'level_set_name_zh' | 'author'> | null,
): PublishZipIssue | null {
  if (inspection.set_count !== 1) return 'multiple'
  if (!isVersionText(inspection.info_version.trim())) return 'version'
  if (identity && zipIdentityMismatch(identity, inspection)) return 'identity'
  return null
}

/**
 * 写操作被 GitHub 拒绝时的结构化结果。
 *
 * 403 有两个完全不同的原因：令牌权限不够（`x-accepted-github-permissions`
 * 给出缺什么）与写入限流。前者要让作者去改 App 权限并重新授权，后者等一会
 * 重试即可 —— 界面必须分开引导，不能只甩一句 GitHub 的英文原文。
 */
export interface PublishPermissionFailure {
  /** 后端返回的完整 tag（含参数），便于日志与重试判定。 */
  tag: string
  /** GitHub 要求的权限，形如 `contents=write`；限流时为空串。 */
  permission: string
  /** 限流解除时间（`x-ratelimit-reset` 的 epoch 秒），权限失败时为空串。 */
  retryAfter: string
  rateLimited: boolean
}

const PERMISSION_TAG = 'mapPublish.missingPermission'
const RATE_LIMIT_TAG = 'mapPublish.rateLimited'

/** 解析 `tag?k=v&k2=v2`；参数值由后端百分号编码（`contents=write` → `contents%3Dwrite`）。 */
function parseParams(query: string): Map<string, string> {
  const params = new Map<string, string>()
  for (const pair of query.split('&')) {
    const mark = pair.indexOf('=')
    if (mark <= 0) continue
    const key = pair.slice(0, mark)
    const raw = pair.slice(mark + 1)
    try {
      params.set(key, decodeURIComponent(raw))
    } catch {
      params.set(key, raw)
    }
  }
  return params
}

/**
 * 从后端错误里认出「权限不足 / 限流」。
 *
 * 只认这两个 tag：其余失败（401、404、422……）仍走普通通知，不能因为一条
 * 写失败就把常驻引导横幅亮出来。
 */
export function parsePublishPermissionFailure(raw: unknown): PublishPermissionFailure | null {
  if (typeof raw !== 'string') return null
  const text = raw.trim()
  const mark = text.indexOf('?')
  const tag = mark === -1 ? text : text.slice(0, mark)
  if (tag !== PERMISSION_TAG && tag !== RATE_LIMIT_TAG) return null
  const params = parseParams(mark === -1 ? '' : text.slice(mark + 1))
  const permission = params.get('permission') ?? ''
  // 权限横幅要显示缺的是哪一项；没有参数说明这不是后端那条分类（或日志被
  // 截断），按普通失败处理，别亮一个空白引导。
  if (tag === PERMISSION_TAG && !permission) return null
  return {
    tag,
    permission,
    retryAfter: params.get('reset') ?? '',
    rateLimited: tag === RATE_LIMIT_TAG,
  }
}

export type PublishPermissionScope = 'contents' | 'administration' | 'metadata' | 'other'

/**
 * GitHub 的权限名 → 语言表里的固定 key 后缀。
 *
 * 发布页要的三项权限是 Contents / Administration / Metadata；GitHub 还可能
 * 报别的（比如 `workflows=write`），统一落到 other，文案只显示原始字符串。
 */
export function permissionScopeKey(permission: string): PublishPermissionScope {
  const text = permission.toLowerCase()
  if (text.includes('contents')) return 'contents'
  if (text.includes('administration')) return 'administration'
  if (text.includes('metadata')) return 'metadata'
  return 'other'
}

/**
 * 排序表的一行 = 一批资产的版本号。
 *
 * `mapsetVersion` 写进 `asset_versions`（末项 = 最新），`infoVersion` 取自 ZIP 里的
 * info，管理器按它判断有没有新版。表里不存附件名：附件名由资产规则渲染得到。
 */
export interface AssetVersionRow {
  mapsetVersion: string
  infoVersion: string
  size: number
}

/**
 * 排序表拖动：入参与返回值都是**存储顺序**（旧→新），下标是**显示顺序**
 * （最新在最上，即存储的反向）。
 *
 * 只在这里做一次反转，页面与组件都不自己算：两处各反转一次会互相抵消，
 * 拖动看起来完全没反应。原数组不修改。
 */
export function reorderVersionRows<T>(rows: readonly T[], fromDisplay: number, toDisplay: number): T[] {
  const display = [...rows].reverse()
  if (fromDisplay < 0 || fromDisplay >= display.length) return [...rows]
  const to = Math.max(0, Math.min(display.length - 1, toDisplay))
  if (fromDisplay === to) return [...rows]
  const moved = display[fromDisplay]
  if (moved === undefined) return [...rows]
  display.splice(fromDisplay, 1)
  display.splice(to, 0, moved)
  return display.reverse()
}
