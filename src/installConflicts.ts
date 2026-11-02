/**
 * 同身份冲突的「怎么卸一项」—— MOD 安装与地图导入共用的唯一实现。
 *
 * 后端在预览/暂存阶段就把同身份的已安装项列进 `conflicts`；按顺序执行、
 * 失败留在原地只重试剩余项，这些都由共享的「安装前检查」弹窗
 * （`InstallCheckDialog`）负责。弹窗不认识 MOD 也不认识地图，它把每一行交回
 * 宿主注入的 `execute`，宿主能复用的恰好是「用哪个 IPC 卸、参数怎么给」——
 * 就是下面这两个函数。
 *
 * 独立成文件而不是写进两个调用方：两个域的卸载语义不同又都很容易写错 ——
 * MOD 要 `allow_unmanaged = true` 走整包语义，地图要把已安装项的启用状态
 * 一起带过去（决定删 `levels/` 还是 `disabled/`）。写两份迟早会分叉。
 *
 * 只依赖 `services/tauri.ts` 的 IPC 封装，不含任何 UI。
 */
import { api } from './services/tauri'
import type { InstallConflict, OperationResult } from './types'

/**
 * 「后端返回 ok=false」统一变成异常。
 *
 * `uninstall_local_mod` / `delete_level_set` 都是 `Result<OperationResult, String>`：
 * 真正的错误走 Err（`catch` 接得住），可预期失败则回 `ok: false` + message tag。
 * 对安装流程来说两者都是「没卸掉」，必须共用同一个失败出口 —— 共享弹窗按
 * 「抛没抛」判定这一行成没成，所以这里一律转成 throw。
 */
async function requireOk(result: Promise<OperationResult>): Promise<void> {
  const outcome = await result
  if (!outcome.ok) throw new Error(outcome.message)
}

/**
 * 卸载冲突的本地 MOD。
 *
 * 冲突卸载按整包语义处理，所以直接 `allowUnmanaged = true`：这里不需要再弹
 * 「整包删除」二级确认 —— 用户已经在「安装前检查」里看过冲突清单并点过
 * 「全部处理」，再问一次只是把同一个意思说两遍。
 */
export function uninstallModConflict(conflict: InstallConflict): Promise<void> {
  return requireOk(api.uninstallLocalMod(conflict.handle, true))
}

/** 卸载冲突的已安装关卡集：句柄是目录名，`enabled` 决定去 levels 还是 disabled。 */
export function uninstallMapConflict(conflict: InstallConflict): Promise<void> {
  return requireOk(api.deleteLevelSet(conflict.handle, conflict.enabled))
}
