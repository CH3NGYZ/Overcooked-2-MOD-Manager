/**
 * 发布页三步式流程的纯逻辑。
 *
 * 作者发一张地图要依次做三件事：①建/改 Release 基本信息 ②上传这一版的 ZIP
 * ③把 oc2-map 块写回 Release note。第三步是**唯一**写块的地方 —— 前两步只写
 * Release 字段与附件，最后一步才对消费端生效，所以顺序不能省。
 *
 * 纯模块（不依赖 Vue / i18n / 组件）：`tests/map-publish-flow.test.mjs` 直接
 * import；页面与 composable 只负责发请求与显示。
 */

// 带 `.ts` 扩展名：Node 的 `--test` 直接跑源码（类型剥离不做补全），Vite 与
// `allowImportingTsExtensions` 都接受这种写法。
import { isVersionText, levelSetSlug } from './mapPublishAsset.ts'

export type PublishStepId = 'release' | 'asset' | 'metadata'

/** 执行顺序：没有 Release 就没有地方挂附件，元数据也要落在已有 Release 上。 */
export const PUBLISH_STEP_ORDER: readonly PublishStepId[] = ['release', 'asset', 'metadata']

/**
 * 面板顺序：ZIP 在最上面 —— 新地图的 tag 与版本号都从 ZIP 名里拆，先选包再填
 * 发布信息。执行顺序仍是 `PUBLISH_STEP_ORDER`，两者不能合并：界面先看到的
 * 不一定是能先跑的。
 */
export const PUBLISH_PANEL_ORDER: readonly PublishStepId[] = ['asset', 'release', 'metadata']

export interface PublishInputState {
  /** 已确定的 tag（新地图从 ZIP 名来，已有地图用现有 tag）。 */
  tag: string
  /** 规范化后的资产文件名；空 = 还没选到合规的 ZIP。 */
  assetName: string
  /** 这一版的版本号文本（允许 `3.4.1-fix` 这种后缀）。 */
  versionText: string
  /** oc2-map 块里的关卡集英文名，写块时必须非空。 */
  levelSetName: string
}

/**
 * 「一键按顺序执行」还缺哪些输入。
 *
 * 只判作者手上的输入，**不判 Release 是否已存在**：执行时第一步本身就去建它，
 * 按「步骤是否已经做完」判会让新建页永远点不动一键执行。每一步自己的前置
 * （Release 存在、ZIP 已上传……）由各自的执行函数再校验一次。
 *
 * 返回顺序跟 `PUBLISH_STEP_ORDER`（执行顺序），不是面板顺序。
 */
export function missingPublishInputs(input: PublishInputState): PublishStepId[] {
  const missing: PublishStepId[] = []
  if (!input.tag.trim()) missing.push('release')
  if (!input.assetName.trim() || !isVersionText(input.versionText)) missing.push('asset')
  if (!input.levelSetName.trim()) missing.push('metadata')
  return missing
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
