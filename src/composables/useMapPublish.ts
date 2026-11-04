import { computed, onUnmounted, ref } from 'vue'
import { api } from '../services/tauri'
import { t } from '../i18n'
import { defaultTemplate, isMapVersion, renderAssetName } from '../mapPublishAsset'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { useNotify } from '../useNotify'
import type {
  MapPublishMetadata,
  PublishAuthPoll,
  PublishAuthState,
  PublishAuthorization,
  PublishRelease,
  PublishRepository,
  PublishZipInspection,
} from '../types'

/**
 * GitHub 授权的界面阶段。
 *
 * `pending` 只是「还没结果」，不单独建态；`authorized` 会立刻切到已登录卡片；
 * `failed` 承载后端 `PublishAuthPoll.failed` 里带参数的完整 tag。
 */
export type AuthPhase = 'idle' | 'waiting' | 'expired' | 'denied' | 'failed'

export interface SaveReleaseInput {
  releaseId: number | null
  tag: string
  title: string
  body: string
  draft: boolean
  prerelease: boolean
  metadata: MapPublishMetadata
}

export interface CreateRepositoryInput {
  name: string
  description: string
  private: boolean
}

/**
 * 某一版要上传的附件名 = 元数据模板 + 这一版的版本号。
 *
 * tag 是地图身份、不参与命名；版本号只存在于附件名里（管理器用
 * `versioned_asset_rule` 从附件名抽），所以必须显式传版本号。渲染规则在
 * `mapPublishAsset.ts`，与发布页的预览共用一份。
 */
export function assetNameFor(release: PublishRelease, version: string): string {
  return renderAssetName(
    release.metadata?.asset ?? '',
    release.metadata?.level_set_name ?? '',
    version,
  )
}

/** 这个 Release 还没有 `oc2-map` 块时的空元数据（新增第一份资产时补上）。 */
function blankMetadata(): MapPublishMetadata {
  return {
    level_set_name: '',
    level_set_name_zh: '',
    author: '',
    name_i18n: {},
    group_i18n: {},
    note_i18n: {},
    asset: '',
    latest_mapset_version: '',
    latest_info_version: '',
    requires: {},
    compatible: {},
    conflicts: {},
  }
}

/**
 * 地图发布页的状态机。
 *
 * 状态刻意留在页面实例上（不做模块级单例）：地图作者只是偶尔来发一版，
 * 登录态与仓库列表重新进页面再读一次最省心，也避免退出登录后别的页面
 * 还拿着旧账号的仓库。
 */
export function useMapPublish() {
  const message = useNotify()

  const auth = ref<PublishAuthState | null>(null)
  const authReady = ref(false)
  const authLoading = ref(false)
  const signingIn = ref(false)
  const signingOut = ref(false)
  const authorization = ref<PublishAuthorization | null>(null)
  const authPhase = ref<AuthPhase>('idle')
  const authFailure = ref('')

  const repositories = ref<PublishRepository[]>([])
  const repositoriesLoading = ref(false)
  const selectedFullName = ref<string | null>(null)
  const creatingRepository = ref(false)

  const releases = ref<PublishRelease[]>([])
  const releasesLoading = ref(false)
  const savingRelease = ref(false)
  const deletingRelease = ref(false)

  const uploading = ref(false)
  const deletingAsset = ref(false)
  /** 「新增资产」弹窗里正在识别 ZIP（解析 `info` 可能要几十秒）。 */
  const inspecting = ref(false)
  /** 上传附件 + 写回 Release note 的整段。 */
  const publishing = ref(false)

  /** 授权到本 App 的安装数量；0 表示还没装过 App。 */
  const installationCount = ref(0)
  /** 有安装范围是「全部仓库」时，新建的仓库才自动在授权范围内。 */
  const allRepositories = ref(false)

  let pollTimer: ReturnType<typeof setTimeout> | null = null
  /** 每次 `clearPoll` 递增：卸载 / 重新登录后迟到的轮询结果必须丢弃。 */
  let pollEpoch = 0
  /** 切仓库时丢弃上一份 Release 请求的迟到结果。 */
  let releasesEpoch = 0
  const authenticated = computed(() => auth.value?.authenticated === true)
  const selectedRepository = computed(
    () => repositories.value.find((item) => item.full_name === selectedFullName.value) ?? null,
  )

  /**
   * 已登录但一个安装都没有 = GitHub App 还没装到任何仓库上。
   *
   * 这时的空列表不是「账号里没有仓库」，而是「还没选仓库范围」：界面要给安装这一步，
   * 而不是让用户去新建一个（新建的仓库也不在授权范围里，推不上去）。
   *
   * 第一次装 App 也走 `startSignIn()`：后端打开的会是安装页，装完 GitHub 回调
   * 本机带回一次 `installation_id`，不靠轮询 `/user/installations`。
   */
  const needsInstall = computed(
    () => authenticated.value && authReady.value && installationCount.value === 0,
  )

  function fail(reason: unknown) {
    message.error(backendMessageOf(reason))
  }

  function clearPoll() {
    pollEpoch += 1
    if (pollTimer !== null) {
      clearTimeout(pollTimer)
      pollTimer = null
    }
  }

  // 离开页面必须让后端关掉本机回调端口；否则下一次登录会因端口被占用失败。
  onUnmounted(() => {
    clearPoll()
    void api.cancelGithubAuthorization().catch(() => {})
  })

  /** 后端要求前端每 1 秒查一次回调结果；间隔由协议固定，不需要后端下发。 */
  const POLL_INTERVAL_MS = 1000

  function schedulePoll() {
    clearPoll()
    const epoch = pollEpoch
    pollTimer = setTimeout(() => {
      pollTimer = null
      void pollAuthorization(epoch)
    }, POLL_INTERVAL_MS)
  }

  async function pollAuthorization(epoch: number) {
    if (epoch !== pollEpoch) return
    let status: PublishAuthPoll
    try {
      status = await api.pollGithubAuthorization()
    } catch (reason) {
      if (epoch !== pollEpoch) return
      // 一次网络失败就结束本次登录：回调最长等 120 秒，反复重试只会让用户
      // 盯着一个不再变化的界面，不如给明确失败并让他重来。
      clearPoll()
      authorization.value = null
      authPhase.value = 'failed'
      authFailure.value = backendMessageOf(reason)
      message.error(authFailure.value)
      return
    }
    if (epoch !== pollEpoch) return

    if (status.status === 'pending') {
      schedulePoll()
      return
    }
    if (status.status === 'authorized') {
      // 安装页那条路回调回来时已经带回 installation_id，列仓库按 id 直查，
      // 不用再等 `/user/installations` 的传播窗口；提示语也跟着换。
      const mode = authorization.value?.mode
      clearPoll()
      authorization.value = null
      authPhase.value = 'idle'
      authFailure.value = ''
      await refreshAuth()
      await loadRepositories()
      if (mode === 'install' && repositories.value.length > 0) {
        message.success(
          t('mapPublish.installationDetected', { count: repositories.value.length }),
        )
      } else {
        message.success(t('mapPublish.signInSuccess'))
      }
      return
    }
    clearPoll()
    authorization.value = null
    if (status.status === 'expired' || status.status === 'denied') {
      authPhase.value = status.status
      return
    }
    authPhase.value = 'failed'
    authFailure.value = translateBackendMessage(status.reason)
    message.error(authFailure.value)
  }

  async function refreshAuth() {
    try {
      auth.value = await api.mapPublishState()
    } catch (reason) {
      fail(reason)
    }
  }

  async function loadReleases() {
    const repository = selectedRepository.value
    if (!repository) {
      releases.value = []
      return
    }
    const epoch = ++releasesEpoch
    releasesLoading.value = true
    try {
      const list = await api.listPublishReleases(repository.owner, repository.name)
      if (epoch === releasesEpoch) releases.value = list
    } catch (reason) {
      if (epoch === releasesEpoch) {
        releases.value = []
        fail(reason)
      }
    } finally {
      if (epoch === releasesEpoch) releasesLoading.value = false
    }
  }

  /** 进页面：先读本地登录态（不联网），已登录再拉仓库与版本。 */
  async function loadState() {
    if (authLoading.value) return
    authLoading.value = true
    try {
      auth.value = await api.mapPublishState()
      if (auth.value.authenticated) await loadRepositories()
    } catch (reason) {
      fail(reason)
    } finally {
      authReady.value = true
      authLoading.value = false
    }
  }

  /** 写入仓库列表并纠正选中项：普通刷新与安装完成后的检测共用一份落地逻辑。 */
  async function adoptRepositories(list: PublishRepository[]) {
    repositories.value = list
    const keep = selectedFullName.value
    if (!keep || !list.some((item) => item.full_name === keep)) {
      selectedFullName.value = list[0]?.full_name ?? null
    }
    await loadReleases()
  }

  async function loadRepositories() {
    if (repositoriesLoading.value) return
    repositoriesLoading.value = true
    try {
      const list = await api.listPublishRepositories()
      installationCount.value = list.installations
      allRepositories.value = list.all_repositories
      await adoptRepositories(list.repositories)
    } catch (reason) {
      fail(reason)
    } finally {
      repositoriesLoading.value = false
    }
  }

  /**
   * 打开 GitHub App 安装页：只在「已经装过、想改仓库范围」时用。
   *
   * 第一次装 App 不走这里 —— 那样 GitHub 不会回调本机，拿不到 `installation_id`
   * 也就不知道用户刚勾了哪些仓库；第一次要走 `startSignIn()`，由后端打开安装页。
   */
  async function openInstallPage() {
    const url = auth.value?.install_url
    if (!url) return
    try {
      const result = await api.openReleasePage(url)
      if (!result.ok) message.error(translateBackendMessage(result.message))
    } catch (reason) {
      fail(reason)
    }
  }

  async function refreshRepositories() {
    await loadRepositories()
  }

  async function selectRepository(fullName: string) {
    if (selectedFullName.value === fullName) return
    selectedFullName.value = repositories.value.some((item) => item.full_name === fullName)
      ? fullName
      : null
    releases.value = []
    await loadReleases()
  }

  async function beginAuthorization(forceAuthorize: boolean) {
    if (signingIn.value) return
    // 重新登录前先取消上一次：后端只保留一个待授权流程，旧监听会占着端口。
    cancelSignIn()
    signingIn.value = true
    try {
      authorization.value = await api.startGithubAuthorization(forceAuthorize)
      authPhase.value = 'waiting'
      schedulePoll()
    } catch (reason) {
      authPhase.value = 'idle'
      fail(reason)
    } finally {
      signingIn.value = false
    }
  }

  /** 主入口：没记下 installation id 时后端打开的是 App 安装页。 */
  function startSignIn() {
    return beginAuthorization(false)
  }

  /** 「改用 GitHub 登录」：安装页对已装账号不回调，这条兜底走纯 OAuth。 */
  function startSignInAnyway() {
    return beginAuthorization(true)
  }

  /** 取消一次进行中的授权（用户主动取消、退出登录、离开页面时都走这里）。 */
  function cancelSignIn() {
    clearPoll()
    authorization.value = null
    authPhase.value = 'idle'
    authFailure.value = ''
    void api.cancelGithubAuthorization().catch(() => {})
  }

  async function signOut() {
    if (signingOut.value) return
    signingOut.value = true
    try {
      auth.value = await api.signOutMapPublish()
      cancelSignIn()
      repositories.value = []
      selectedFullName.value = null
      releases.value = []
      releasesEpoch += 1
      installationCount.value = 0
      allRepositories.value = false
      message.success(t('mapPublish.signedOut'))
    } catch (reason) {
      fail(reason)
    } finally {
      signingOut.value = false
    }
  }

  async function createRepository(input: CreateRepositoryInput): Promise<PublishRepository | null> {
    if (creatingRepository.value) return null
    creatingRepository.value = true
    try {
      const created = await api.createPublishRepository(
        input.name.trim(),
        input.private,
        input.description.trim(),
      )
      repositories.value = [created, ...repositories.value.filter((item) => item.full_name !== created.full_name)]
      selectedFullName.value = created.full_name
      await loadReleases()
      message.success(t('mapPublish.repositoryCreated'))
      return created
    } catch (reason) {
      fail(reason)
      return null
    } finally {
      creatingRepository.value = false
    }
  }

  async function saveRelease(
    input: SaveReleaseInput,
    options: { quiet?: boolean } = {},
  ): Promise<PublishRelease | null> {
    const repository = selectedRepository.value
    if (!repository || savingRelease.value) return null
    savingRelease.value = true
    try {
      const saved = await api.savePublishRelease({
        owner: repository.owner,
        repository: repository.name,
        release_id: input.releaseId,
        tag: input.tag,
        title: input.title,
        body: input.body,
        draft: input.draft,
        prerelease: input.prerelease,
        metadata: input.metadata,
      })
      if (!options.quiet) message.success(t('mapPublish.saved'))
      await loadReleases()
      return saved
    } catch (reason) {
      fail(reason)
      return null
    } finally {
      savingRelease.value = false
    }
  }

  async function deleteRelease(release: PublishRelease, deleteTag: boolean): Promise<boolean> {
    const repository = selectedRepository.value
    if (!repository || deletingRelease.value) return false
    deletingRelease.value = true
    try {
      const result = await api.deletePublishRelease({
        owner: repository.owner,
        repository: repository.name,
        release_id: release.id,
        tag: release.tag,
        delete_tag: deleteTag,
      })
      if (!result.ok) {
        message.error(translateBackendMessage(result.message))
        return false
      }
      message.success(translateBackendMessage(result.message) || t('mapPublish.deleted'))
      releases.value = releases.value.filter((item) => item.id !== release.id)
      return true
    } catch (reason) {
      fail(reason)
      return false
    } finally {
      deletingRelease.value = false
    }
  }

  async function uploadAsset(release: PublishRelease, path: string, version: string): Promise<boolean> {
    const repository = selectedRepository.value
    if (!repository || uploading.value) return false
    const versionText = version.trim()
    if (!versionText) {
      message.warning(t('mapPublish.missingVersion'))
      return false
    }
    if (!isMapVersion(versionText)) {
      message.warning(t('mapPublish.invalidVersion', { value: versionText }))
      return false
    }
    const name = assetNameFor(release, versionText)
    if (!name) {
      // 模板里没有版本占位符：传上去管理器也抽不出版本，先拦住。
      message.warning(t('mapPublish.assetNeedsVersion'))
      return false
    }
    uploading.value = true
    try {
      await api.uploadPublishAsset({
        owner: repository.owner,
        repository: repository.name,
        release_id: release.id,
        path,
        name,
      })
      message.success(t('mapPublish.uploaded'))
      await loadReleases()
      return true
    } catch (reason) {
      fail(reason)
      return false
    } finally {
      uploading.value = false
    }
  }


  /** 识别本地 ZIP：`info` 解析是重活，后端在 spawn_blocking 里跑。 */
  async function inspectZip(path: string): Promise<PublishZipInspection | null> {
    if (inspecting.value) return null
    inspecting.value = true
    try {
      return await api.inspectPublishZip(path)
    } catch (reason) {
      fail(reason)
      return null
    } finally {
      inspecting.value = false
    }
  }

  /**
   * 「新增资产」：上传 ZIP 附件，再把这一版的版本号写回 Release note。
   *
   * 两步的顺序不能反：先落附件（消费端按附件名抽版本），再改 note 的
   * `latest_mapset_version`。note 写失败时附件已经传上去了，所以只提示
   * 「附件已传、说明没更新」，并把列表刷新出来让作者手动重试。
   */
  async function publishVersion(
    release: PublishRelease,
    path: string,
    version: string,
    inspection: PublishZipInspection,
  ): Promise<boolean> {
    const repository = selectedRepository.value
    if (!repository || publishing.value) return false
    const versionText = version.trim()
    if (!versionText) {
      message.warning(t('mapPublish.missingVersion'))
      return false
    }
    if (!isMapVersion(versionText)) {
      message.warning(t('mapPublish.invalidVersion', { value: versionText }))
      return false
    }
    const base = release.metadata ?? blankMetadata()
    // 老 Release 可能还没有关卡集名（第一次上传），用这次识别出来的补上，
    // 否则默认模板会渲染成 `-1.2.0.zip`。
    const levelSetName = base.level_set_name.trim() || inspection.level_set_name.trim()
    const name = renderAssetName(base.asset, levelSetName, versionText)
    if (!name) {
      message.warning(t('mapPublish.assetNeedsVersion'))
      return false
    }
    publishing.value = true
    try {
      await api.uploadPublishAsset({
        owner: repository.owner,
        repository: repository.name,
        release_id: release.id,
        path,
        name,
      })
      const saved = await saveRelease(
        {
          releaseId: release.id,
          tag: release.tag,
          title: release.title,
          body: release.body,
          draft: release.draft,
          prerelease: release.prerelease,
          metadata: {
            ...base,
            level_set_name: levelSetName,
            level_set_name_zh: base.level_set_name_zh.trim() || inspection.level_set_name_zh.trim(),
            author: base.author.trim() || inspection.author.trim(),
            asset: base.asset.trim() || defaultTemplate(levelSetName),
            latest_mapset_version: versionText,
            latest_info_version: inspection.info_version.trim() || base.latest_info_version,
            requires:
              Object.keys(base.requires).length > 0 ? base.requires : inspection.requires,
          },
        },
        { quiet: true },
      )
      if (!saved) {
        await loadReleases()
        message.warning(t('mapPublish.publishedWithoutNote', { name }))
        return false
      }
      await loadReleases()
      message.success(t('mapPublish.versionPublished', { version: versionText }))
      return true
    } catch (reason) {
      fail(reason)
      return false
    } finally {
      publishing.value = false
    }
  }

  async function deleteAsset(assetId: number): Promise<boolean> {
    const repository = selectedRepository.value
    if (!repository || deletingAsset.value) return false
    deletingAsset.value = true
    try {
      const result = await api.deletePublishAsset(repository.owner, repository.name, assetId)
      if (!result.ok) {
        message.error(translateBackendMessage(result.message))
        return false
      }
      message.success(translateBackendMessage(result.message) || t('mapPublish.deleted'))
      await loadReleases()
      return true
    } catch (reason) {
      fail(reason)
      return false
    } finally {
      deletingAsset.value = false
    }
  }

  return {
    auth,
    authReady,
    authLoading,
    authenticated,
    needsInstall,
    installationCount,
    allRepositories,
    openInstallPage,
    signingIn,
    signingOut,
    authorization,
    authPhase,
    authFailure,
    repositories,
    repositoriesLoading,
    selectedFullName,
    selectedRepository,
    creatingRepository,
    releases,
    releasesLoading,
    savingRelease,
    deletingRelease,
    uploading,
    deletingAsset,
    inspecting,
    publishing,
    loadState,
    startSignIn,
    startSignInAnyway,
    cancelSignIn,
    signOut,
    refreshRepositories,
    selectRepository,
    createRepository,
    saveRelease,
    deleteRelease,
    uploadAsset,
    deleteAsset,
    inspectZip,
    publishVersion,
  }
}
