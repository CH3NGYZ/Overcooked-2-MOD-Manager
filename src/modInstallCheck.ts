/**
 * MOD 安装的「同身份冲突 → 安装前检查」纯映射。
 *
 * 为什么不写进 `useModInstallFlow.ts`：那一层 import 了 Vue 与 `services/tauri`
 * （浏览器专用），Node 单测直接 import 会炸；这里只做数据搬运，能像
 * `installDetail.ts` / `dropAccept.ts` 一样被 `node --test` 直接跑。
 *
 * 这一层存在的理由是两个弹窗的分工：
 *   · 后端在预览阶段就把「本地已有同身份副本」列进 `ModPreview.conflicts`
 *     （MOD 同 GUID 或同名，见 `mod_preview::find_install_conflicts`）；
 *   · 检查弹窗负责把这些项卸掉，**卸完才允许**进入安装详情弹窗 ——
 *     用户在检查那一屏就能看到是谁没卸成，而不是在详情里点了「卸载并安装」
 *     才发现装不了（旧实现在确认按钮里才卸，卸载失败时弹窗内容已经说不清了）；
 *   · 安装详情弹窗不再展示「还要卸载谁」：冲突已经处理完，`conflicts` 清空后
 *     它的警示块与「卸载并安装」按钮文案自然回到「立即安装」。
 *
 * `withConflictsResolved` 因此不是「藏起错误」，而是如实表达「这些冲突已经被
 * 处理掉了」：后端 `confirm_mod_install` 不复查冲突（`mod_preview.rs` 只按来源
 * 记录分流到仓库安装 / 手动安装），清空后安装不会因为冲突再失败。
 */
import type { ModPreview } from './types'

/**
 * 这份预览需不需要先弹安装前检查。
 *
 * 无冲突时**不弹空窗**（与 `useInstallPlanGate` 同一条原则）：绝大多数安装都
 * 属于这种，每次都弹一个「没问题」的窗口只会让用户学会无脑点掉它。
 */
export function needsInstallCheck(preview: Pick<ModPreview, 'conflicts'>): boolean {
  return (preview.conflicts?.length ?? 0) > 0
}

/**
 * 检查弹窗里的「目标名」。
 *
 * 用包名而不是文件名：用户在卡片与仓库里认的是包名，文件名只在「我下载的是
 * 哪个文件」这个问题下才有意义。两者都空时退回空串（后端不会产出这种预览，
 * 但 `package_name` 在类型上是 string，仍要有确定行为）。
 */
export function modCheckTarget(preview: Pick<ModPreview, 'package_name' | 'file_name'>): string {
  return preview.package_name?.trim() || preview.file_name?.trim() || ''
}

/**
 * 冲突已处理完的预览副本：把 `conflicts` 清空，其余字段与引用原样保留。
 *
 * 只做浅拷贝：`entries` 可能有很多条，深拷贝没有意义（安装详情只读它）；
 * 而且调用方拿到的是一份新对象，不会把「已处理」这个结论写回后端返回的原值。
 */
export function withConflictsResolved<T extends Pick<ModPreview, 'conflicts'>>(preview: T): T {
  return { ...preview, conflicts: [] }
}
