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
import { pickI18n, t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { beginDownloadTask, endDownloadTask, failDownloadTask } from '../downloadTask'
import { isPrereleaseVersion } from '../modStatus'
import type { ModPreview, Repository } from '../types'

export interface RepositoryPreviewOptions {
  /** 游戏是否在运行；返回 true 时中止并已给出提示。 */
  gameBlocked?: () => boolean
  /** 下载开始/结束的忙碌回调，用于驱动各自卡片的 loading。 */
  onBusyChange?: (repositoryId: string, busy: boolean) => void
  /**
   * 安装前的**关系检查并处理**：依赖、冲突、兼容一次跑完。
   *
   * 返回 `true` 表示都处理好了、可以继续安装主包；`false` 表示用户取消、
   * 有关系处理失败、或存在无法自动处理的问题，此时**不应**继续装主包 ——
   * 装一个跑不起来、或会和已装内容冲突的 MOD，比不装更糟。
   *
   * 为什么是三合一而不是三个回调：这些关系的解法互相影响
   * （冲突要先卸、版本要先换、依赖最后补），拆开就变成了三段互不知情的
   * 流程，宿主还得自己排顺序。宿主只实现「弹出计划、执行、告诉我结果」，
   * 顺序与判定都留在 `modRelations.buildInstallPlan`。
   */
  prepareInstall?: (repo: Repository, tag?: string) => Promise<boolean>
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
   * 顺序：**预发布确认 → 关系检查（依赖/冲突/兼容）→ 下载**。
   *
   * 预发布确认必须最先问：用户可能只想要正式版，拒了就到此为止。
   * 关系检查会真的卸载/安装东西，把它排在确认之前，等于拿用户的环境
   * 去试错 —— 可能已经卸掉了冲突 MOD，才发现用户根本不要这个测试版。
   * 下载仍放在最后，避免用户中途拒绝时白白下载整个安装包。
   *
   * `assetName` 是详情页资产下拉里选中的文件；不传（卡片上的安装按钮）时
   * 后端按仓库 `asset_rules` 自动选。传了就必须能匹配到，否则后端报
   * `noMatchingAsset` —— 界面显示哪个就下哪个，不静默换成别的。
   */
  async function previewRepository(repo: Repository, tag?: string, assetName?: string): Promise<void> {
    if (options.gameBlocked?.()) return

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
    /*
     * 预发布确认必须排在关系检查**之前**。
     *
     * 用户可能只想装正式版：先问「这是预览版，确定吗」，拒了就到此为止，
     * 不该先为他弹一个「安装前检查」、甚至已经卸掉了冲突 MOD 才发现他
     * 根本不要这个测试版。确认意图在前，代价高昂的副作用在后 ——
     * 关系检查会真的卸载/安装东西，顺序反了就是拿用户的环境去试错。
     */
    if (prerelease) {
      const confirmed = await new Promise<boolean>((resolve) => {
        dialog.warning({
          title: t('modsLocal.preReleaseVersion'),
          content: t('modsLocal.thisIsAPreReleaseBuildAndMayHaveUnknownIssuesContinueInstalling'),
          positiveText: t('modsLocal.continueInstalling'),
          negativeText: t('recycle.cancel'),
          onPositiveClick: () => resolve(true),
          onNegativeClick: () => resolve(false),
          onClose: () => resolve(false),
          onMaskClick: () => resolve(false),
        })
      })
      if (!confirmed) return
    }

    /*
     * 安装前先处理依赖 / 冲突 / 兼容。
     *
     * 顺序很重要：这些关系不先理顺，主包装上也不可用 ——
     * 缺依赖跑不起来、冲突可能直接崩游戏、版本不兼容同样白装。
     * 等装完再提示，用户已经离开这个页面，中间那段时间还是个
     * 「装了但不工作」的误导状态。
     *
     * 检查要针对**这次要装的版本**：不同大版本可以声明不同关系
     * （1.0 依赖 B≥2，1.1 依赖 B≥3），所以把 tag 一起传下去。
     */
    if (options.prepareInstall) {
      const ready = await options.prepareInstall(repo, tag)
      // 没处理好就不继续：装一个跑不起来或冲突的 MOD 比不装更糟
      if (!ready) return
    }

    /*
     * 下载这一段单独成函数：失败卡片上的「重试」要能原样再跑一次它。
     * 预发布确认与关系检查不跟着重跑 —— 前者是用户意图，后者会真的
     * 卸载/安装东西，它们在下载失败之前就已经做完了。
     */
    const download = async (): Promise<void> => {
      const token = beginDownloadTask({
        label: pickI18n(repo.name_i18n, repo.name),
        retry: download,
      })
      options.onBusyChange?.(repo.id, true)
      try {
        const outcome = await api.installRepository(repo.id, tag, true, assetName)
        // 走到这里说明 IPC 本身成功了：ok/message 是业务结论，不是下载故障
        endDownloadTask(token)
        if (!outcome.ok) {
          message.error(translateBackendMessage(outcome.message))
          return
        }
        if (!outcome.preview) {
          message.error(t('modsOnline.downloadFinishedButNoInstallableContentWasParsed'))
          return
        }
        openPreview(outcome.preview)
      } catch (reason) {
        // 后端的下载失败一律走 Err：这是唯一需要「原地重试」的情形
        failDownloadTask(token, reason)
        message.error(backendMessageOf(reason))
      } finally {
        options.onBusyChange?.(repo.id, false)
      }
    }

    await download()
  }

  return { previewRepository }
}
