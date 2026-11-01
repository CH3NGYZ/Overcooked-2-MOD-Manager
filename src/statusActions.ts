import type { TagTone } from './theme'

/**
 * MOD 卡片上的「状态快捷动作」。
 *
 * 卡片标签区左侧那个按钮不是固定某个动作，而是**由当前最需要注意的状态
 * 决定**：可更新时是「更新」、仓库刷新失败时是「刷新」。以后再加状态
 * （例如「版本未知」需要重新检查）只需要在 `STATUS_ACTIONS` 里加一行，
 * 模板与包标题都不用改。
 *
 * 这里只描述「该显示什么」，不执行动作 —— 真正的 handler 由视图提供，
 * 因为不同页面（MOD 管理 / MOD 安装）执行同一个动作的方式不同：
 * 前者要预下载再弹安装详情，后者可能只刷新一条仓库的 Release。
 */

/** 动作种类：决定调用哪个 handler。 */
export type StatusActionKind =
  | 'update'
  | 'refresh'
  | 'install-dependency'
  | 'uninstall-conflict'

export interface StatusActionSpec {
  kind: StatusActionKind
  /** 按钮文案的 i18n key（同时也是 icon 选择依据）。 */
  label: string
  /**
   * 视觉档位。与触发它的标签同档，这样「可更新 + 更新按钮」和
   * 「刷新失败 + 刷新按钮」各自是同一语义色，不会两个按钮用同一个色
   * 却表达不同紧急度。
   */
  tone: TagTone
}

/**
 * 状态 → 快捷动作。
 *
 * 只登记「用户看到就想立刻处理」的状态：
 *   - `update_available` → 更新（有新版可装）
 *   - `repository_unavailable` → 刷新（已安装 MOD 的绑定仓库这轮没刷出来）
 *   - `refresh_failed` → 刷新（MOD 安装页仓库卡片自己的刷新失败）
 *   - `bound_repository_missing` → 刷新（仓库从配置里消失，重拉源可能找回）
 *
 * 刻意不登记的状态：
 *   - `unknown` / `refreshing`：还在查（或查完也判断不出），刷新不一定有改善；
 *   - `duplicate` / `load_warning`：要用户自己删文件或改环境，不是一键动作；
 *   - `disabled` / `prerelease` / `latest`：无需动作。
 */
const STATUS_ACTIONS: Record<string, StatusActionSpec> = {
  update_available: { kind: 'update', label: 'dialogs.update', tone: 'moderate' },
  repository_unavailable: { kind: 'refresh', label: 'modsOnline.refresh', tone: 'moderate' },
  refresh_failed: { kind: 'refresh', label: 'modsOnline.refresh', tone: 'moderate' },
  bound_repository_missing: { kind: 'refresh', label: 'modsOnline.refresh', tone: 'critical' },
  /*
   * 缺依赖 → 「去安装」。色调用 critical，与「缺依赖」标签同档 ——
   * 用户看到的标签与按钮是同一种颜色，不需要额外解释它们是一回事。
   */
  missing_dependency: { kind: 'install-dependency', label: 'overview.browse', tone: 'critical' },
  /*
   * 冲突 → 「去卸载」。与「冲突」标签同档同色。
   *
   * 冲突不能靠安装解决 —— 装什么都不会让「你不该装它」变成「该装」，
   * 唯一出路是卸掉。所以这里指向卸载确认，而不是仓库跳转。
   */
  dependency_conflict: { kind: 'uninstall-conflict', label: 'modsOnline.uninstall', tone: 'critical' },
}

/**
 * 标签列表里可能同时有多件该处理的事，按钮位只有一个，按这里选。
 *
 * 不能按标签传入顺序选：那个顺序是**严重度**排序，缺依赖（critical）
 * 会排在可更新（moderate）前面，于是用户看到「去安装」而真正该先做的
 * 是更新。这里的优先级独立表达「先给哪个动作」：
 *
 *   uninstall-conflict 冲突可能已经让游戏崩溃，先解决它
 *   update            用户本来就想做的事，做完可能连依赖也一起换了
 *   install-dependency 让 MOD 能跑起来，比刷新更接近「解决问题」
 *   refresh           只是重新取数据，不直接改变可用性
 */
const ACTION_PRIORITY: StatusActionKind[] = [
  'uninstall-conflict',
  'update',
  'install-dependency',
  'refresh',
]

/**
 * 按状态标签列表解析出唯一的快捷动作。
 *
 * 同一标签列表里可能有多个可执行动作，按 `ACTION_PRIORITY` 取最优先的；
 * 同一 kind 只保留第一次出现（不需要再比较，标签已按严重度排好）。
 */
export function statusActionFor(labels: readonly string[] | undefined): StatusActionSpec | undefined {
  const available = new Map<StatusActionKind, StatusActionSpec>()
  for (const label of labels ?? []) {
    const action = STATUS_ACTIONS[label]
    if (action && !available.has(action.kind)) available.set(action.kind, action)
  }
  for (const kind of ACTION_PRIORITY) {
    const action = available.get(kind)
    if (action) return action
  }
  return undefined
}
