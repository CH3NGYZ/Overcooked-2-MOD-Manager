/**
 * 「安装前检查」弹窗的宿主接线 —— 地图导入与 MOD 安装共用一份。
 *
 * 三种来源都要做同一件事：把模型摆上去、等人点完、拿一个布尔结论继续后面的
 * 流程。这段接线写三遍的话，「弹窗被 Esc / 关闭按钮收掉时也要 resolve」这种
 * 边界迟早会在某一份里漏掉 —— 漏了的后果是导入/安装流程永远挂在 await 上，
 * 入口按钮从此转圈。
 *
 * 用法（宿主视图里挂一个 `<InstallCheckDialog>`）：
 *
 * ```ts
 * const check = useInstallCheck()
 * const ready = await check.run(model, async (row) => { await doSomething(row) })
 * ```
 *
 * 弹窗只负责「按顺序跑完所有行」，跑哪一行由这里注入的 `execute` 决定：
 * 同一个模型既可能来自 MOD 关系计划，也可能来自地图冲突，弹窗分不出域，
 * 也就不该猜。
 */
import { ref, watch } from 'vue'
import type { InstallCheckModel, InstallCheckRow } from '../installCheck'

export function useInstallCheck() {
  /** 弹窗开关，直接绑 `v-model:show`。 */
  const open = ref(false)
  /** 本次要处理的内容。 */
  const model = ref<InstallCheckModel | null>(null)
  /** 本次每行的执行器（由 `run` 注入，`execute` 转发给它）。 */
  const executor = ref<((row: InstallCheckRow) => Promise<void>) | null>(null)

  let settle: ((ready: boolean) => void) | null = null

  function resolve(ready: boolean) {
    const finish = settle
    settle = null
    finish?.(ready)
  }

  /**
   * 弹出检查窗口并等结论。
   *
   * 返回 `true` = 所有行都处理成功且没有只能提醒的条目；`false` = 用户取消、
   * 仍有失败、或存在无法自动处理的提醒。
   *
   * 上一轮还没结论就要开新窗时，先把上一轮当作放弃：否则那边 await 的流程
   * 会永远挂着，`importing` / `installing` 之类的忙碌标志也再也回不去。
   */
  function run(
    next: InstallCheckModel,
    execute: (row: InstallCheckRow) => Promise<void>,
  ): Promise<boolean> {
    resolve(false)
    model.value = next
    executor.value = execute
    open.value = true
    return new Promise<boolean>((resolveRun) => { settle = resolveRun })
  }

  /** 弹窗给出结论时调用（全部处理完 / 取消）。 */
  function finish(ready: boolean) {
    resolve(ready)
  }

  /** 绑在弹窗 `:execute` 上：转发给本次 `run` 注入的执行器。 */
  async function execute(row: InstallCheckRow) {
    const action = executor.value
    if (!action) throw new Error('install check has no executor')
    await action(row)
  }

  /*
   * 弹窗被任何方式收起（叉掉、Esc、宿主自己关）都算放弃。
   * 正常关闭时 `finish` 已经先 resolve 过，这里只会命中「没有结论就关了」。
   */
  watch(open, (visible) => { if (!visible) resolve(false) })

  return { open, model, run, finish, execute }
}
