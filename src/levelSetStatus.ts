import { compareVersions } from './modStatus.ts'
import type { LevelSetInfo } from './types'

/**
 * 关卡集（地图）的状态判定。
 *
 * ## 与 MOD 的区别
 *
 * 结构上很像，但**身份来源不同**：
 *
 * - MOD 的身份是 BepInPlugin 的 GUID，由 BepInEx 加载，能直接参与插件依赖关系；
 * - 关卡集的身份是 info 里的 `levelSetName + author`，由 DIYLevel 读取，
 *   与插件依赖体系无关。
 *
 * 所以两者共用 `compareVersions` 这类纯工具，但状态机各写一份 —— 硬把
 * 「本地关卡集」塞进 `InstalledMod` 会让每个使用点都要判断「这是哪种」。
 */

/** 关卡集的展示状态。 */
export type LevelSetState =
  /** 已停用（在 `disabled/` 里）。 */
  | 'disabled'
  /** info 解析失败：名称与版本都读不到，但目录仍然可用（能开关）。 */
  | 'unreadable'
  /** info 里的版本比仓库声明的版本低。 */
  | 'update_available'
  /** 与仓库版本一致。 */
  | 'latest'
  /** 本地版本比远端还高（作者的预发布，或仓库 tag 回退）。 */
  | 'ahead'
  /** 没有绑定任何地图仓库（本地导入的，或仓库已下线）。 */
  | 'unbound'
  /** 无法比较版本（任一侧缺失）。 */
  | 'unknown'

/** 地图仓库对某个关卡集的版本声明。 */
export interface MapVersionRef {
  /** Release tag（例如 `v0.73`）。 */
  tag: string
  /** 关卡集名（与 info 的 `levelSetName` 比对）。 */
  levelSetName: string
  /** 作者（与 info 的 `author` 比对）。 */
  author: string
}

/**
 * 判断两个关卡集是否是「同一个地图」。
 *
 * 用 `author + levelSetName`，**不用 uid**：uid 是 `MD5(baseUID + version)`，
 * 每发一版就变，拿它当身份会把每次更新都认成新地图。
 *
 * 比较忽略大小写与首尾空白 —— 作者在不同工具里导出时大小写常不一致。
 */
export function isSameLevelSet(
  left: { name: string; author: string },
  right: { name: string; author: string },
): boolean {
  const normalize = (value: string) => value.trim().toLowerCase()
  return normalize(left.name) === normalize(right.name) && normalize(left.author) === normalize(right.author)
}

/**
 * 从一个关卡集与其绑定的仓库版本，算出展示状态。
 *
 * `remote` 为 `undefined` 表示没有绑定（本地导入）或仓库还没刷新。
 */
export function levelSetState(
  set: Pick<LevelSetInfo, 'enabled' | 'version' | 'error'>,
  remote?: MapVersionRef | null,
): LevelSetState {
  // 停用优先：用户关掉的东西不该显示「可更新」，那会催他去更新一个不用的地图
  if (!set.enabled) return 'disabled'
  if (set.error) return 'unreadable'
  if (!remote) return 'unbound'
  const order = compareVersions(set.version, remote.tag)
  if (order === null) return 'unknown'
  if (order < 0) return 'update_available'
  if (order > 0) return 'ahead'
  return 'latest'
}

/** 状态 → 展示用的 tag（与 `modsLocal.*` 的命名保持同一套语义）。 */
export const LEVEL_SET_STATE_TAGS: Record<LevelSetState, string> = {
  disabled: 'modsLocal.disabled',
  unreadable: 'tools.versionUnknown',
  update_available: 'tools.update',
  latest: 'modsLocal.latest',
  ahead: 'modsLocal.latest',
  unbound: 'modsLocal.unbound',
  unknown: 'tools.versionUnknown',
}

/** 状态 → 语义色调，与 MOD 侧共用同一档位。 */
export const LEVEL_SET_STATE_TONES: Record<
  LevelSetState,
  'accent' | 'moderate' | 'muted' | 'critical'
> = {
  disabled: 'muted',
  unreadable: 'critical',
  update_available: 'moderate',
  latest: 'accent',
  ahead: 'accent',
  unbound: 'muted',
  unknown: 'muted',
}

/**
 * 找一个关卡集对应的仓库版本声明。
 *
 * 返回第一个身份匹配的仓库的 tag。多个仓库声明同一个关卡集属于配置错误，
 * 这里不猜哪个「更对」，按传入顺序取第一个 —— 与 MOD 侧 `repositoryForMod`
 * 的行为保持一致。
 */
export function mapVersionForLevelSet(
  set: Pick<LevelSetInfo, 'name' | 'author'>,
  refs: readonly MapVersionRef[],
): MapVersionRef | undefined {
  if (!set.name.trim()) return undefined
  return refs.find((ref) => isSameLevelSet(set, { name: ref.levelSetName, author: ref.author }))
}

/**
 * 关卡集的展示名：优先中文名，其次英文名，最后退回目录名。
 *
 * 与 MOD 的 `pickI18n` 不同：这里没有多语言表，只有「中英两个字段 + 目录名」
 * 三级回退，因为 info 里就只存了这两项。
 */
export function levelSetDisplayName(
  set: Pick<LevelSetInfo, 'name' | 'name_zh' | 'dir_name'>,
  locale: string,
): string {
  const preferChinese = locale.startsWith('zh')
  const chinese = set.name_zh?.trim()
  const english = set.name.trim()
  if (preferChinese && chinese) return chinese
  if (english) return english
  if (chinese) return chinese
  return set.dir_name
}
