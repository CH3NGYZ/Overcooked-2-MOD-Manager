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
export type StatusActionKind = 'update' | 'refresh'

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
 *   - `unknown` / `checking`：还在查或查完也判断不出，刷新不一定有改善；
 *   - `duplicate` / `load_warning`：要用户自己删文件或改环境，不是一键动作；
 *   - `disabled` / `prerelease` / `latest`：无需动作。
 */
const STATUS_ACTIONS: Record<string, StatusActionSpec> = {
  update_available: { kind: 'update', label: '更新', tone: 'moderate' },
  repository_unavailable: { kind: 'refresh', label: '刷新', tone: 'moderate' },
  refresh_failed: { kind: 'refresh', label: '刷新', tone: 'moderate' },
  bound_repository_missing: { kind: 'refresh', label: '刷新', tone: 'critical' },
}

/**
 * 按状态标签列表解析出唯一的快捷动作。
 *
 * 一个 MOD / 一个包可能同时带着多条标签（例如「可更新」+「重复」），
 * 但按钮位只有一个，所以按传入顺序取**第一个**有动作的状态 ——
 * 调用方传入的顺序本身按严重度排好了（与标签展示顺序一致），
 * 因此这里不需要再排序，也不会出现两个按钮抢同一个位置。
 */
export function statusActionFor(labels: readonly string[] | undefined): StatusActionSpec | undefined {
  for (const label of labels ?? []) {
    const action = STATUS_ACTIONS[label]
    if (action) return action
  }
  return undefined
}
