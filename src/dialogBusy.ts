import type { DialogReactive } from 'naive-ui'

/**
 * 让一个已经弹出的确认框进入「正在执行」状态。
 *
 * naive-ui 的 `dialog.warning({ onPositiveClick })` 只会等 Promise 结束再关闭
 * 弹窗，**不会**自己给确定按钮加 loading：用户点了「移入回收站」之后，如果
 * 回收站写入要几百毫秒，按钮和一秒前一模一样，看不出点击已生效 —— 于是再点
 * 一次。这里按 naive-ui 自己暴露的 `DialogOptions.loading` 走（`Dialog.mjs`
 * 把它同时接到确定按钮的 `loading` 与 `disabled`），并顺手锁住其余关闭路径，
 * 保证操作期间不会被 Esc / 遮罩 / 右上角叉关掉，也不会点到「取消」。
 *
 * 为什么不包一层新的 confirm helper：现有弹窗的 `onPositiveClick` 里有「二次
 * 确认」「失败后保留弹窗让用户原地重试」这类分支，把它们再挪进一层只会更难读；
 * 这里只加一个显式动作，调用点仍然是原来的 `dialog.warning(...)`。
 */
export function markDialogBusy(instance: DialogReactive): void {
  instance.loading = true
  instance.closable = false
  instance.maskClosable = false
  instance.negativeButtonProps = { ...instance.negativeButtonProps, disabled: true }
}
