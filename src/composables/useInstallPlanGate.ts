/**
 * 安装前检查的宿主实现：生成计划、弹窗、等结果。
 *
 * 两个页面都要做同一件事 —— MOD 安装页的仓库卡片、MOD 管理页的「更新」——
 * 所以这段接线只能有一份。之前只有安装页有它，管理页的「更新」直接跳过
 * 检查去下载，于是「更新一个新增了依赖的版本」会装上一个跑不起来的包。
 *
 * 组件的职责边界：
 *   · 本 composable 负责「什么时候弹、弹什么、等结果」；
 *   · `modRelations.buildInstallPlan` 负责「哪些能自动处理」；
 *   · `InstallPlanDialog` 负责「展示与按顺序执行」。
 * 三者都不该知道另外两个的实现细节。
 */
import { ref, watch } from 'vue'
import { pickI18n } from '../i18n'
import {
  buildInstallPlan, planHasActions, type InstallPlan,
} from '../modRelations'
import type { InstalledMod, Repository } from '../types'

export interface InstallPlanGateOptions {
  /** 已装 MOD（判定依赖与冲突的对象）。 */
  mods: () => readonly InstalledMod[]
  /** 当前可用的在线仓库（找可安装来源）。 */
  repositories: () => readonly Repository[]
  /** 游戏是否在运行；返回 true 时中止并已给出提示。 */
  gameBlocked: () => boolean
  /**
   * 计划可能改动磁盘（卸载/安装），结束后调用方应刷新数据。
   *
   * 只在计划里真的含 uninstall 或 install 时才调用 —— 纯提醒（blocked）
   * 不改任何文件，没必要重扫整个游戏目录。
   */
  onApplied: () => void | Promise<void>
}

export function useInstallPlanGate(options: InstallPlanGateOptions) {
  /** 弹窗开关与内容。 */
  const open = ref(false)
  const plan = ref<InstallPlan>({ uninstall: [], install: [], blocked: [] })
  const target = ref('')
  /**
   * 弹窗结果：`null` = 还在等；`true` = 可以继续；`false` = 取消或失败。
   * 用 ref + Promise 而不是回调，因为调用方是 `await` 的。
   */
  const outcome = ref<boolean | null>(null)

  /**
   * 生成计划并等用户处理完。
   *
   * 没有需要处理的关系时直接返回 `true`，**不弹空窗** —— 绝大多数安装
   * 都属于这种，每次都弹一个「没问题」的窗口只会让用户学会无脑点掉它，
   * 真有问题那次也会被一起点掉。
   *
   * `releaseVersion` 是这次要装的版本：`release_relations` 按它筛关系表，
   * 用它而不是「已装版本」—— 用户要装的是新版本，规则也该按新版本算。
   */
  async function run(repo: Repository, tag?: string): Promise<boolean> {
    const version = tag ?? repo.cached_releases[0]?.tag_name
    const built = buildInstallPlan(
      repo,
      options.mods(),
      options.repositories(),
      version,
    )
    if (!planHasActions(built)) return true
    // A map itself may be installed while the game runs, but a plan that
    // changes MOD files (such as an unmet framework dependency) is not.
    if (options.gameBlocked()) return false

    plan.value = built
    target.value = pickI18n(repo.name_i18n, repo.name)
    outcome.value = null
    open.value = true
    return new Promise<boolean>((resolve) => {
      const stop = watch(outcome, (value) => {
        if (value === null) return
        stop()
        resolve(value)
      })
    })
  }

  /** 弹窗给出结果时调用（全部处理完 / 用户取消 / 仍然安装）。 */
  async function finish(ready: boolean) {
    outcome.value = ready
    // 计划里可能卸载/装过东西，磁盘已变，调用方必须重读
    if (plan.value.uninstall.length || plan.value.install.length) {
      await options.onApplied()
    }
  }

  return { open, plan, target, run, finish }
}
