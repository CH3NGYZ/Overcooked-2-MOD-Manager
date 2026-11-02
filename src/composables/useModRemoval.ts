/**
 * 卸载本地 MOD 的统一流程（含后端要求的二次确认）。
 *
 * 「删除 MOD」在三个地方出现：MOD 管理的卡片、MOD 管理的包标题、
 * MOD 安装页的「卸载冲突的 MOD」。三处除了标题和文案不同，后续流程完全一样：
 *
 *   1. 弹第一层确认（文案随「删除后进回收站」开关变化）
 *   2. 调 `uninstall_local_mod`
 *   3. 后端可能返回两种需要再确认的结果：
 *        requires_package_confirmation —— 该 DLL 属于多文件 MOD 包
 *        requires_confirmation         —— 共享依赖 / 同 GUID 等风险
 *      两者都要先弹「整包删除」二级确认，只有都不需要才真正删除
 *   4. 刷新 Dashboard
 *
 * 这段流程之前只写在 MOD 管理页里，冲突卸载另写了一份简化版 ——
 * 那份缺少包确认分支，遇到包内 DLL 会直接报错。抽到一处后，
 * 任何新入口只要提供文案就能获得完整的确认链。
 */
import { useDialog } from 'naive-ui'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { markDialogBusy } from '../dialogBusy'
import type { InstalledMod } from '../types'

export interface RemovalCopy {
  /** 确认框标题。 */
  title: string
  /** 确认框正文；参数是该 MOD 的显示名。 */
  body: (name: string) => string
}

export interface ModRemovalOptions {
  /**
   * 删除文案（随「删除后进回收站」设置变化）。
   *
   * 必须与真实行为一致：关掉回收站后删除就是抹掉文件，
   * 再提示「已移入回收站」会让人以为还能找回。
   */
  deleteCopy: () => { body: (name: string) => string; action: string; done: string }
  /** MOD 的显示名，用于确认文案。 */
  titleOf: (mod: InstalledMod) => string
  /** 忙碌标记（可选）：宿主用它驱动对应卡片的 loading。 */
  onBusy?: (mod: InstalledMod, busy: boolean) => void
  /** 卸载成功后刷新数据。 */
  onDone: () => void | Promise<void>
}

export function useModRemoval(options: ModRemovalOptions) {
  const dialog = useDialog()
  const message = useNotify()

  /**
   * 卸载一个本地 MOD。
   *
   * @param mod 目标 MOD（冲突卸载时它是「对方」，不是当前卡片那一项）
   * @param copy 覆盖默认文案；不传就是普通删除
   */
  function remove(mod: InstalledMod, copy?: RemovalCopy) {
    const fallback = options.deleteCopy()
    const instance = dialog.warning({
      title: copy?.title ?? t('modsLocal.deleteMod'),
      content: copy ? copy.body(options.titleOf(mod)) : fallback.body(options.titleOf(mod)),
      positiveText: fallback.action,
      negativeText: t('recycle.cancel'),
      async onPositiveClick() {
        markDialogBusy(instance)
        options.onBusy?.(mod, true)
        try {
          const result = await api.uninstallLocalMod(mod.dll_file)
          if (result.requires_package_confirmation || result.requires_confirmation) {
            // 后端此时还没删任何东西（ok=false），确认后才带 allow_unmanaged 重试
            const packageInstance = dialog.warning({
              title: t('modsLocal.thisModBelongsToAPackage'),
              content: t('modsLocal.itSharesAPackageWithModsDeleteThemTogether', {
                count: result.package_mod_names?.length ?? 0,
              }),
              positiveText: t('modsLocal.deletePackage'),
              negativeText: t('recycle.cancel'),
              async onPositiveClick() {
                markDialogBusy(packageInstance)
                const packageResult = await api.uninstallLocalMod(mod.dll_file, true)
                packageResult.ok
                  ? message.success(translateBackendMessage(packageResult.message) || fallback.done)
                  : message.error(translateBackendMessage(packageResult.message))
                await options.onDone()
              },
            })
          } else {
            result.ok
              ? message.success(translateBackendMessage(result.message) || fallback.done)
              : message.error(translateBackendMessage(result.message))
          }
          await options.onDone()
        } catch (reason) {
          message.error(backendMessageOf(reason))
        } finally {
          options.onBusy?.(mod, false)
        }
      },
    })
  }

  return { remove }
}
