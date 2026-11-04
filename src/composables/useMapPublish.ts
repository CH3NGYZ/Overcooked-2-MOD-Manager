import { computed, onUnmounted, ref } from 'vue'
import { api } from '../services/tauri'
import { t } from '../i18n'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { useNotify } from '../useNotify'
import type {
  MapPublishMetadata,
  PublishAuthPoll,
  PublishAuthState,
  PublishAuthorization,
  PublishRelease,
  PublishRepository,
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
 * 把元数据里的附件模板渲染成这一版的真实附件名。
 *
 * 后端保存时若 `asset` 为空会生成 `<slug>-${version}.zip`，管理器再用
 * `versioned_asset_rule` 把模板变成按版本匹配的正则；上传的附件名必须与
 * 渲染结果完全一致，否则发布出去的地图在管理器里找不到 ZIP。
 */
export function assetNameFor(release: PublishRelease): string {
  const template = release.metadata?.asset.trim() ?? ''
  const fallback = `${release.tag}.zip`
  if (!template) return fallback
  return safeAssetName(template.split('${version}').join(release.tag), fallback)
}

/** 附件名只允许 `A-Z a-z 0-9 . _ -`，其余字符替换成 `-`（GitHub 查询串会变形）。 */
function safeAssetName(name: string, fallback: string): string {
  const cleaned = name
    .replace(/[^A-Za-z0-9._-]+/g, '-')
    .replace(/-{2,}/g, '-')
    .replace(/^[.-]+|[.-]+$/g, '')
    .slice(0, 200)
  return cleaned || fallback
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

  let pollTimer: ReturnType<typeof setTimeout> | null = null
  /** 每次 `clearPoll` 递增：卸载 / 重新登录后迟到的轮询结果必须丢弃。 */
  let pollEpoch = 0
  /** 切仓库时丢弃上一份 Release 请求的迟到结果。 */
  let releasesEpoch = 0

  const authenticated = computed(() => auth.value?.authenticated === true)
  const selectedRepository = computed(
    () => repositories.value.find((item) => item.full_name === selectedFullName.value) ?? null,
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
      // 一次网络失败就结束本次登录：回调最长等 5 分钟，反复重试只会让用户
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
      clearPoll()
      authorization.value = null
      authPhase.value = 'idle'
      authFailure.value = ''
      message.success(t('mapPublish.signInSuccess'))
      await refreshAuth()
      await loadRepositories()
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

  async function loadRepositories() {
    if (repositoriesLoading.value) return
    repositoriesLoading.value = true
    try {
      const list = await api.listPublishRepositories()
      repositories.value = list
      const keep = selectedFullName.value
      if (!keep || !list.some((item) => item.full_name === keep)) {
        selectedFullName.value = list[0]?.full_name ?? null
      }
      await loadReleases()
    } catch (reason) {
      fail(reason)
    } finally {
      repositoriesLoading.value = false
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

  async function startSignIn() {
    if (signingIn.value) return
    // 重新登录前先取消上一次：后端只保留一个待授权流程，旧监听会占着端口。
    cancelSignIn()
    signingIn.value = true
    try {
      authorization.value = await api.startGithubAuthorization()
      authPhase.value = 'waiting'
      schedulePoll()
    } catch (reason) {
      authPhase.value = 'idle'
      fail(reason)
    } finally {
      signingIn.value = false
    }
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

  async function saveRelease(input: SaveReleaseInput): Promise<PublishRelease | null> {
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
      message.success(t('mapPublish.saved'))
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

  async function uploadAsset(release: PublishRelease, path: string): Promise<boolean> {
    const repository = selectedRepository.value
    if (!repository || uploading.value) return false
    uploading.value = true
    try {
      await api.uploadPublishAsset({
        owner: repository.owner,
        repository: repository.name,
        release_id: release.id,
        path,
        name: assetNameFor(release),
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
    loadState,
    startSignIn,
    cancelSignIn,
    signOut,
    refreshRepositories,
    selectRepository,
    createRepository,
    saveRelease,
    deleteRelease,
    uploadAsset,
    deleteAsset,
  }
}
