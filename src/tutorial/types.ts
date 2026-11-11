import type { TutorialPlacement } from './geometry'

/**
 * 内置教程的步骤模型。
 *
 * ## 为什么步骤在前端而不是 TOML
 *
 * V1 把步骤写成 `resources/builtin-tutorial.toml`，靠 `selector` 字符串定位元素，
 * 于是「这一步要先点开抽屉、再等列表渲染出来」这种真实交互完全表达不了 ——
 * 只能靠 `scenario` 造一份假界面副本，既不准也维护不动。V2 的步骤直接是
 * TypeScript：要开抽屉就写 `prepare`，要跨页就写 `route`，文案走语言表。
 *
 * ## 目标怎么定位
 *
 * `target` 优先用页面上的 `data-tutorial="<id>"`（约定见 `.tmp/tutorial-contract.md`）。
 * 也接受函数形式：返回多个元素时取并集 —— 标题栏那排按钮、页头两个按钮
 * 这类「一整组一起讲」的步骤用得着。
 *
 * 找不到目标不是错误：`optional` 的步骤直接跳过（列表为空、没有绑定仓库时
 * 那些区域本来就不渲染），非 optional 的步骤退化成居中卡片继续讲。
 */
export interface TutorialStep {
  /** 全教程唯一。`startTutorial({ fromStepId })` 与测试都按它定位。 */
  id: string
  /** 标题文案 key（`tutorial` 域的完整 tag，如 `tutorial.chrome.zoom.title`）。 */
  titleKey: string
  /** 正文文案 key。 */
  bodyKey: string
  /** 高亮目标；缺省 = 不挖洞，卡片居中显示（纯讲解步骤）。 */
  target?: string | (() => Element | Element[] | null | undefined)
  /** 目标不存在时跳过整步（默认 false：退化成居中卡片）。 */
  optional?: boolean
  /** 进入本步前先切到该路由（vue-router 的 name）。 */
  route?: string
  /** 进入本步前的准备动作：开抽屉、展开卡片、切 tab 等。 */
  prepare?: (ctx: TutorialContext) => void | Promise<void>
  /** 离开本步（上一步/下一步/跳过）时的收尾：关抽屉、收起卡片。 */
  cleanup?: (ctx: TutorialContext) => void | Promise<void>
  /** 卡片宽度（px），默认 380，运行时再按视口收窄。 */
  cardWidth?: number
  /** 高亮框向外扩的像素，默认 6。 */
  padding?: number
  /** 卡片相对目标的方位，默认 auto（下 → 上 → 右 → 左 → 居中）。 */
  placement?: TutorialPlacement
  /** 只在满足条件时把本步编进本次教程（例如「有本地 MOD」才讲卡片按钮）。 */
  when?: () => boolean
}

/** 播放顺序与管理目录共用同一份章节登记；步骤 id 保持全局唯一。 */
export interface TutorialSection {
  id: string
  titleKey: string
  steps: readonly TutorialStep[]
}

/**
 * `prepare` / `cleanup` 拿到的工具集。
 *
 * 只给「点一下、等一等」这几件事，不给 router / store：步骤负责描述交互，
 * 真正的状态与命令仍在页面里，教程只是替用户点一下。
 *
 * 所有 `waitFor*` 都带超时并返回结果，不做静默死等 —— 目标没出现时
 * 教程要能继续往下讲，而不是卡在那里。
 */
export interface TutorialContext {
  /** 点第一个命中的元素；没命中返回 false（不抛错）。 */
  click(selector: string, root?: ParentNode): boolean
  /** 等到元素出现，超时返回 null。 */
  waitFor(selector: string, timeoutMs?: number, root?: ParentNode): Promise<Element | null>
  /** 等到元素消失（关抽屉用），超时返回 false。 */
  waitForGone(selector: string, timeoutMs?: number, root?: ParentNode): Promise<boolean>
  /** 等固定时长（用于等过渡动画）。 */
  wait(ms: number): Promise<void>
  /** 当前路由 name；跨页步骤判断用。 */
  currentRoute(): string | null
}
