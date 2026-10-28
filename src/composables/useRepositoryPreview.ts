/**
 * 仓库资产的「先下载预览，再确认安装」。
 *
 * MOD 安装页的仓库卡片和 MOD 管理页的「更新」按钮都要做同一件事：
 *   1. 让后端以 preview_only 下载并解析资产（走正常的多源竞速与代理选择）
 *   2. 把解析结果交给统一的 ModInstallDialog
 *   3. 用户确认后才真正写入游戏目录
 *
 * 两个页面的差异只有 UI 侧的忙碌状态和提示方式，所以这里只保留
 * 「校验 → 下载 → 交预览」这一段，两页各自决定 loading 显示在哪里。
 */
import { useDialog } from 'naive-ui'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { t } from '../i18n'
import { isPrereleaseVersion } from '../modStatus'
import type { ModPreview, Repository } from '../types'

export interface RepositoryPreviewOptions {
  /** 游戏是否在运行；返回 true 时中止并已给出提示。 */
  gameBlocked?: () => boolean
  /** 下载开始/结束的忙碌回调，用于驱动各自卡片的 loading。 */
  onBusyChange?: (repositoryId: string, busy: boolean) => void
}

/**
 * @param openPreview 拿到预览后由宿主打开统一的安装详情弹窗
 */
export function useRepositoryPreview(openPreview: (preview: ModPreview) => void, options: RepositoryPreviewOptions = {}) {
  const message = useNotify()
  const dialog = useDialog()

  /**
   * 预览一份仓库资产。
   *
   * 预发布版本先确认再下载 —— 确认放在下载之前，
   * 避免用户拒绝后白白下载一整个安装包。
   */
  async function previewRepository(repo: Repository, tag?: string): Promise<void> {
    if (options.gameBlocked?.()) return

    const target = tag ?? repo.cached_releases[0]?.tag_name
    const release = repo.cached_releases.find((entry) => entry.tag_name === target)
    // 预发布判定：元数据标记 或 版本号带 - 后缀（0.4.0-beta / 0.4.0.5-beta）
    const prerelease = release
      ? isPrereleaseVersion(release.version || release.tag_name, release.prerelease)
      : false
    if (prerelease) {
      const confirmed = await new Promise<boolean>((resolve) => {
        dialog.warning({
          title: t('预发布版本'),
          content: t('此版本为预览版本，可能存在未知问题，是否继续安装？'),
          positiveText: t('继续安装'),
          negativeText: t('取消'),
          onPositiveClick: () => resolve(true),
          onNegativeClick: () => resolve(false),
          onClose: () => resolve(false),
          onMaskClick: () => resolve(false),
        })
      })
      if (!confirmed) return
    }

    options.onBusyChange?.(repo.id, true)
    try {
      const outcome = await api.installRepository(repo.id, tag, true)
      if (!outcome.ok) {
        message.error(outcome.message)
        return
      }
      if (!outcome.preview) {
        message.error(t('下载完成但没有解析出安装内容'))
        return
      }
      openPreview(outcome.preview)
    } catch (reason) {
      message.error(reason instanceof Error ? reason.message : String(reason))
    } finally {
      options.onBusyChange?.(repo.id, false)
    }
  }

  return { previewRepository }
}
