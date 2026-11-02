/**
 * 安装冲突的「先卸载、再安装」流程 —— MOD 安装与地图导入共用一份。
 *
 * 后端在预览/暂存阶段就把同身份的已安装项列进 `conflicts`；前端在用户确认
 * 安装后要先把它们卸载（进回收站），全部成功才继续安装新内容。两边各自
 * 卸载失败的处理完全一样（中止、弹错误、弹窗不关），所以「顺序卸载 + 首个
 * 失败即中止」只在这里实现一次，两个调用方只提供「怎么卸一项」。
 *
 * 只依赖 `services/tauri.ts` 的 IPC 封装，不含任何 UI；`uninstallConflicts`
 * 本身不碰 IPC，Node 单测可以直接注入计数器验证执行顺序。
 */
import { backendMessageOf } from './locale/backend'
import { api } from './services/tauri'
import type { InstallConflict, OperationResult } from './types'

export interface ConflictUninstallResult {
  ok: boolean
  /** 第一个卸载失败的冲突项（`ok` 为真时不出现）。 */
  failed?: InstallConflict
  /** 已翻译成界面语言的失败原因。 */
  reason?: string
  /**
   * 还没卸载的冲突项（`ok` 为真时为空）。
   *
   * 失败后用户处理完外部原因（关掉游戏、手动删掉占用的那份）会在**同一个**
   * 确认弹窗上再点一次；那时只能重试这些，已经卸掉的不能重复卸载，否则
   * 第二次一定报「找不到」、用户永远装不上。
   */
  remaining: InstallConflict[]
}

/**
 * 「后端返回 ok=false」统一变成异常。
 *
 * `uninstall_local_mod` / `delete_level_set` 都是 `Result<OperationResult, String>`：
 * 真正的错误走 Err（`catch` 接得住），可预期失败则回 `ok: false` + message tag。
 * 对安装流程来说两者都是「没卸掉」，必须共用同一个失败出口。
 */
async function requireOk(result: Promise<OperationResult>): Promise<void> {
  const outcome = await result
  if (!outcome.ok) throw new Error(outcome.message)
}

/**
 * 顺序卸载一批冲突项，第一个失败就返回，后面的不再调用。
 *
 * 顺序而不是并发：后端卸载共用同一把 operation 锁，并发只会让哪个先失败
 * 变得不可预测，还会在失败后留下一半已卸载、一半没动的中间状态。
 */
export async function uninstallConflicts(
  conflicts: InstallConflict[],
  uninstall: (conflict: InstallConflict) => Promise<void>,
): Promise<ConflictUninstallResult> {
  for (const [index, conflict] of conflicts.entries()) {
    try {
      await uninstall(conflict)
    } catch (reason) {
      return {
        ok: false,
        failed: conflict,
        reason: backendMessageOf(reason),
        remaining: conflicts.slice(index),
      }
    }
  }
  return { ok: true, remaining: [] }
}

/**
 * 卸载冲突的本地 MOD。
 *
 * 冲突卸载按整包语义处理，所以直接 `allowUnmanaged = true`：这里不需要
 * 再弹「整包删除」二级确认，用户已经在安装详情里看过冲突清单并确认过。
 */
export function uninstallModConflict(conflict: InstallConflict): Promise<void> {
  return requireOk(api.uninstallLocalMod(conflict.handle, true))
}

/** 卸载冲突的已安装关卡集：句柄是目录名，`enabled` 决定去 levels 还是 disabled。 */
export function uninstallMapConflict(conflict: InstallConflict): Promise<void> {
  return requireOk(api.deleteLevelSet(conflict.handle, conflict.enabled))
}
