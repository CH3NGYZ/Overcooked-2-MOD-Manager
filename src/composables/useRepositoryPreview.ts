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
  /**
   * 安装前检查依赖：返回需要同时安装的依赖仓库（按安装顺序）。
   *
   * 返回空数组表示没有缺失或没有可安装来源，直接走正常安装。
   * 为什么放在「安装前」而不是「装完再提示」：用户点安装的意图是
   * 「让它能用」，缺依赖时它装上也跑不起来；等装完再弹一个窗，
   * 用户已经离开这个页面了。
   */
  dependenciesForInstall?: (repo: Repository) => Repository[]
  /**
   * 批量安装依赖。宿主提供实现（它知道怎么下载预览和弹确认窗）。
   *
   * 返回 `true` 表示依赖已装好、可以继续安装主包；`false` 表示用户取消
   * 或有依赖失败，此时**不应**继续装主包 —— 装一个跑不起来的 MOD
   * 比不装更糟。
   *
   * 与 `dependenciesForInstall` 成对出现：前者决定「要装哪些」，
   * 后者负责「怎么装」。两个都提供时才启用前置依赖安装。
   */
  installDependencies?: (repo: Repository, dependencies: Repository[]) => Promise<boolean>
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

    /*
     * 安装前先补齐依赖。
     *
     * 顺序很重要：依赖必须先装好，主包才有意义 —— 先把主包装上、再提示
     * 缺依赖，用户已经离开这个页面，而且中间那段时间主包是「装了但不工作」
     * 的误导状态。
     *
     * 只有两个回调都提供时才启用（MOD 安装页会提供；MOD 管理页的「更新」
     * 不提供 —— 那里更新的是已装 MOD，依赖在它首次安装时就该齐了）。
     */
    if (options.dependenciesForInstall && options.installDependencies) {
      const dependencies = options.dependenciesForInstall(repo)
      if (dependencies.length) {
        const ready = await options.installDependencies(repo, dependencies)
        // 依赖没装好就不继续：装一个跑不起来的 MOD 比不装更糟
        if (!ready) return
      }
    }

    const target = tag ?? repo.cached_releases[0]?.tag_name
    const release = repo.cached_releases.find((entry) => entry.tag_name === target)
    /*
     * 预发布判定：优先用后端算好的 `prerelease` 标记。
     *
     * 兜底只能传 `tag_name` —— 后端的 `version` 是归一后的三段文本，
     * 第 4 段（beta 序号）已经丢了，拿它反推会把 `1.8.7.23` 判成正式版，
     * 而卡片那边看到的是预发布，两处结论不一致。
     */
    const prerelease = release
      ? isPrereleaseVersion(release.tag_name || release.version, release.prerelease)
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
