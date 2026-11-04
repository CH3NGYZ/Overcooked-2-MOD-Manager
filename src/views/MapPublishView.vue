<script setup lang="ts">
import { computed, h, onMounted, reactive, ref, watch } from 'vue'
import { NButton, NCheckbox, NInput, NPopover, NSpin, NSwitch, NTag, useDialog } from 'naive-ui'
import { RefreshOutline, SettingsOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { api } from '../services/tauri'
import PageLoading from '../components/PageLoading.vue'
import { useFileDrop } from '../composables/useFileDrop'
import { useMapPublish } from '../composables/useMapPublish'
import {
  defaultTemplate,
  isMapVersion,
  latestVersionOf,
  renderAssetName,
  versionFromAssetName,
} from '../mapPublishAsset'
import {
  PUBLISH_STEP_ORDER,
  defaultMapTag,
  missingPublishInputs,
  permissionScopeKey,
  type PublishStepId,
} from '../mapPublishFlow'
import { markDialogBusy } from '../dialogBusy'
import { formatFileSize } from '../repositoryAssets'
import { useNotify } from '../useNotify'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import type { MapPublishMetadata, PublishAsset, PublishRelease, PublishZipInspection } from '../types'

/**
 * 地图发布页 —— 作者把地图 ZIP 作为 GitHub Release 发出去的唯一入口。
 *
 * 口径：**一个 tag 就是一张地图集**，版本号只写在附件名里。列表按地图集
 * 展示；进到某张地图集里才是「地图管理」，发布 / 更新固定走三步：发布信息 →
 * 上传这一版的 ZIP → 写回 oc2-map 信息，最下面是这张地图已有的资产。
 *
 * 与地图下载页的关系：这一页只负责「写 GitHub」，不读地图源。作者保存后，
 * 管理器下一次刷新地图源就会从 Release note 的 oc2-map 块读到这张地图。
 *
 * 页面不放进侧栏（作者是少数用户），入口在工具页。
 */

const message = useNotify()
const dialog = useDialog()

const {
  auth,
  authReady,
  authLoading,
  authenticated,
  needsInstall,
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
  inspecting,
  uploading,
  deletingAsset,
  missingTopics,
  permissionFailure,
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
  inspectZip,
  uploadAsset,
  deleteAsset,
} = useMapPublish()

onMounted(loadState)

/* ===== 视图：列表 / 地图管理（含新建） ===== */

type ViewMode = 'list' | 'manage' | 'create'

const mode = ref<ViewMode>('list')
/** 正在管理的地图集：用 tag 定位，`releases` 刷新后自动跟着换新对象。 */
const activeTag = ref('')
/** 新建模式下第一步创建出来的 Release；第二、三步都挂在它上面。 */
const createdRelease = ref<PublishRelease | null>(null)

/**
 * 当前正在编辑的那张地图集的 Release。
 *
 * 「地图管理」按 tag 去最新列表里找（刷新后拿到的是新对象）；「新建地图集」
 * 在第一步成功前是 null，成功后按创建结果找 —— 后面的上传与写块都需要一个
 * 真实存在的 Release id。
 */
const workingRelease = computed(() => {
  const tag = mode.value === 'manage' ? activeTag.value : (createdRelease.value?.tag ?? '')
  if (!tag) return null
  return releases.value.find((item) => item.tag === tag) ?? createdRelease.value
})

/** 一张地图集「当前算哪一版」：附件名里的最大版本，老 Release 退回 metadata。 */
function latestVersionOfRelease(release: PublishRelease): string {
  return (
    latestVersionOf(release.assets.map((asset) => asset.name)) ||
    release.metadata?.latest_mapset_version.trim() ||
    ''
  )
}

function displayName(release: PublishRelease): string {
  return release.metadata?.level_set_name.trim() || release.title.trim() || release.tag
}

/* ===== 地图信息表单（新建与编信息共用一份） ===== */

interface FormState {
  /** 地图 ID（git tag）：一张地图一个，创建后不能改。 */
  tag: string
  title: string
  body: string
  draft: boolean
  prerelease: boolean
}

/**
 * 元数据里三张 `Record<string, string>` 在界面上是文本域。
 *
 * 一行一条 `key = value` 比表格好写也好粘贴：作者本来就在 info / 源文档里
 * 维护这类清单，直接复制过来即可，不需要逐行点「添加」。
 */
interface MetadataDraft {
  levelSetName: string
  levelSetNameZh: string
  author: string
  asset: string
  latestMapsetVersion: string
  latestInfoVersion: string
  nameI18n: string
  noteI18n: string
  groupI18n: string
  requires: string
  compatible: string
  conflicts: string
}

function blankMetadataDraft(): MetadataDraft {
  return {
    levelSetName: '',
    levelSetNameZh: '',
    author: '',
    asset: '',
    latestMapsetVersion: '',
    latestInfoVersion: '',
    nameI18n: '',
    noteI18n: '',
    groupI18n: '',
    requires: '',
    compatible: '',
    conflicts: '',
  }
}

const form = reactive<FormState>({ tag: '', title: '', body: '', draft: false, prerelease: false })
const metadataDraft = reactive<MetadataDraft>(blankMetadataDraft())

/* GitHub 账号与仓库范围收在页头设置面板里，主区只留当前仓库的地图集。 */
const settingsOpen = ref(false)
const repositoryFilter = ref('')

const filteredRepositories = computed(() => {
  const keyword = repositoryFilter.value.trim().toLowerCase()
  if (!keyword) return repositories.value
  return repositories.value.filter((repository) => repository.full_name.toLowerCase().includes(keyword))
})

function openSettings() {
  settingsOpen.value = true
}

/** 选完仓库收起面板：主区的地图集列表随即换成这个仓库的。 */
function chooseRepository(fullName: string) {
  settingsOpen.value = false
  repositoryFilter.value = ''
  void selectRepository(fullName)
}

/**
 * 解析「一行一条」的清单。
 *
 * 关系值本身带 `>=` / `<=`，所以不能简单按第一个 `=` 切：等号紧跟在比较
 * 运算符后面时属于值，不是分隔符。`GUID >= 4.0.0` 这种没写分隔等号的写法
 * 也收，作者从别处粘贴时常是这一种。
 */
function parsePairs(text: string): Record<string, string> {
  const result: Record<string, string> = {}
  for (const raw of text.split('\n')) {
    const line = raw.trim()
    if (!line || line.startsWith('#')) continue
    let split = -1
    for (let index = 1; index < line.length; index += 1) {
      if (line[index] === '=' && !'=<>~!'.includes(line[index - 1])) {
        split = index
        break
      }
    }
    if (split === -1) {
      const matched = line.match(/^(\S+)\s+([<>=!~].+)$/)
      if (matched) result[matched[1]] = matched[2].trim()
      continue
    }
    const key = line.slice(0, split).trim()
    if (!key) continue
    result[key] = line.slice(split + 1).trim()
  }
  return result
}

/** 版本区间统一写成 `>=4.0.0`（后端解析器接受空格，但存进 block 时没必要保留）。 */
function parseRelations(text: string): Record<string, string> {
  const result: Record<string, string> = {}
  for (const [key, value] of Object.entries(parsePairs(text))) {
    result[key] = value.replace(/^([<>=!~]+)\s+/, '$1')
  }
  return result
}

function pairsToText(record: Record<string, string>): string {
  return Object.entries(record)
    .map(([key, value]) => `${key} = ${value}`)
    .join('\n')
}

function applyMetadata(metadata: MapPublishMetadata | null) {
  const draft = blankMetadataDraft()
  if (metadata) {
    draft.levelSetName = metadata.level_set_name
    draft.levelSetNameZh = metadata.level_set_name_zh
    draft.author = metadata.author
    draft.asset = metadata.asset
    draft.latestMapsetVersion = metadata.latest_mapset_version
    draft.latestInfoVersion = metadata.latest_info_version
    draft.nameI18n = pairsToText(metadata.name_i18n)
    draft.noteI18n = pairsToText(metadata.note_i18n)
    draft.groupI18n = pairsToText(metadata.group_i18n)
    draft.requires = pairsToText(metadata.requires)
    draft.compatible = pairsToText(metadata.compatible)
    draft.conflicts = pairsToText(metadata.conflicts)
  }
  Object.assign(metadataDraft, draft)
}

function resetForm(release: PublishRelease | null, metadata: MapPublishMetadata | null = release?.metadata ?? null) {
  form.tag = release?.tag ?? ''
  form.title = release?.title ?? ''
  form.body = release?.body ?? ''
  form.draft = release?.draft ?? false
  form.prerelease = release?.prerelease ?? false
  applyMetadata(metadata)
}

function buildMetadata(): MapPublishMetadata {
  return {
    level_set_name: metadataDraft.levelSetName.trim(),
    level_set_name_zh: metadataDraft.levelSetNameZh.trim(),
    author: metadataDraft.author.trim(),
    name_i18n: parsePairs(metadataDraft.nameI18n),
    group_i18n: parsePairs(metadataDraft.groupI18n),
    note_i18n: parsePairs(metadataDraft.noteI18n),
    asset: metadataDraft.asset.trim(),
    latest_mapset_version: metadataDraft.latestMapsetVersion.trim(),
    latest_info_version: metadataDraft.latestInfoVersion.trim(),
    requires: parseRelations(metadataDraft.requires),
    compatible: parseRelations(metadataDraft.compatible),
    conflicts: parseRelations(metadataDraft.conflicts),
  }
}

function openManagement(release: PublishRelease) {
  activeTag.value = release.tag
  createdRelease.value = null
  rejectPending()
  lastUploaded.value = ''
  lastFailedStep.value = null
  resetForm(release)
  mode.value = 'manage'
}

/**
 * 新建地图集：从空白开始，靠拖进来的第一个 ZIP 回填名字、作者与依赖。
 *
 * 不从上一张图复制元数据：关卡集名与作者必须是被发布的那张图的，复制过来
 * 一旦忘了改，写出去的块会顶着别人的身份（消费端按身份判断是不是同一张图）。
 */
function openCreate() {
  if (!selectedRepository.value) return
  createdRelease.value = null
  rejectPending()
  lastUploaded.value = ''
  lastFailedStep.value = null
  resetForm(null, null)
  mode.value = 'create'
}

function backToList() {
  mode.value = 'list'
  activeTag.value = ''
  createdRelease.value = null
  rejectPending()
  lastUploaded.value = ''
  lastFailedStep.value = null
  resetForm(null, null)
}

/* 换仓库时旧表单没有意义：tag 与地图信息都属于上一个仓库。 */
watch(selectedFullName, backToList)

/* ===== 三步式：发布信息 → 上传 ZIP → 写回地图信息 ===== */

/** 当前正在跑哪一步；同一时刻只允许一步在跑（一键执行时逐段串起来）。 */
const stepBusy = ref<PublishStepId | null>(null)
/** 「一键按顺序执行」整段在跑：按钮转圈，同时挡掉单独点某一步。 */
const runningAll = ref(false)
/** 上一次失败在哪一步：权限横幅上的「重试这一步」按它重跑。 */
const lastFailedStep = ref<PublishStepId | null>(null)
/** 上一次上传成功后的附件名：步骤 2 留在界面上给作者确认。 */
const lastUploaded = ref('')

const pending = reactive({
  path: '',
  version: '',
  inspection: null as PublishZipInspection | null,
})

const pendingZipName = computed(() => {
  const path = pending.path
  if (!path) return ''
  return path.split(/[\\/]/).pop() ?? path
})

/** 渲染附件名用的关卡集名：表单优先，其次是刚识别出来的。 */
const pendingLevelSetName = computed(
  () => metadataDraft.levelSetName.trim() || pending.inspection?.level_set_name.trim() || '',
)

/** 会上传成什么名字：模板（留空按关卡集名生成）+ 这一版的版本号。 */
const pendingAssetName = computed(() =>
  renderAssetName(metadataDraft.asset, pendingLevelSetName.value, pending.version),
)

const busyAny = computed(
  () =>
    stepBusy.value !== null ||
    runningAll.value ||
    savingRelease.value ||
    uploading.value ||
    inspecting.value,
)

const stepReleaseDone = computed(() => workingRelease.value !== null)
const stepAssetDone = computed(() => lastUploaded.value !== '')
const stepMetadataDone = computed(() => Boolean(workingRelease.value?.metadata))

/**
 * 一键执行还缺的输入；空数组才允许点。
 *
 * 这里**不看** `stepReleaseDone`：新建地图集时第一步正是去建 Release，把它
 * 算进前置会让新建页永远提示「缺发布信息」。
 */
const missingInputs = computed(() =>
  missingPublishInputs({
    tag: form.tag,
    zipReady: pending.path !== '' && pending.inspection !== null,
    assetVersion: pending.version,
    levelSetName: metadataDraft.levelSetName,
  }),
)

function stepTitle(step: PublishStepId): string {
  switch (step) {
    case 'release':
      return t('mapPublish.stepReleaseTitle')
    case 'asset':
      return t('mapPublish.stepAssetTitle')
    case 'metadata':
      return t('mapPublish.stepMetadataTitle')
  }
}

const sequenceHint = computed(() =>
  missingInputs.value.length > 0
    ? t('mapPublish.sequenceNeed', { steps: missingInputs.value.map(stepTitle).join(' / ') })
    : t('mapPublish.sequenceReady'),
)

/** 权限横幅上的权限名：认得出的给中文名，认不出的原样显示。 */
const permissionLabel = computed(() => {
  const failure = permissionFailure.value
  if (!failure) return ''
  switch (permissionScopeKey(failure.permission)) {
    case 'contents':
      return t('mapPublish.permissionScopeContents')
    case 'administration':
      return t('mapPublish.permissionScopeAdministration')
    case 'metadata':
      return t('mapPublish.permissionScopeMetadata')
    default:
      return t('mapPublish.permissionScopeOther', { permission: failure.permission })
  }
})

/** 限流解除时间：GitHub 给的是 epoch 秒，按本机时区显示。 */
const permissionResetText = computed(() => {
  const seconds = Number(permissionFailure.value?.retryAfter ?? '')
  if (!Number.isFinite(seconds) || seconds <= 0) return t('mapPublish.rateLimitedUnknown')
  return new Date(seconds * 1000).toLocaleTimeString()
})

function rejectPending() {
  pending.path = ''
  pending.version = ''
  pending.inspection = null
}

function requiresText(record: Record<string, string>): string {
  return Object.entries(record)
    .map(([key, value]) => `${key} ${value}`)
    .join(', ')
}

/**
 * 识别结果回填表单。
 *
 * 名称 / 作者 / 依赖只在还空着时填：作者可能已经手写过，不能被第二个 ZIP
 * 覆盖；两个版本字段相反，它们描述的就是「这次上传的这一版」，直接覆盖。
 */
function applyInspection(result: PublishZipInspection) {
  if (!form.tag.trim()) form.tag = defaultMapTag(result.level_set_name)
  if (!form.title.trim()) {
    form.title = result.level_set_name.trim() || result.level_set_name_zh.trim()
  }
  if (!metadataDraft.levelSetName.trim() && result.level_set_name.trim()) {
    metadataDraft.levelSetName = result.level_set_name.trim()
  }
  if (!metadataDraft.levelSetNameZh.trim() && result.level_set_name_zh.trim()) {
    metadataDraft.levelSetNameZh = result.level_set_name_zh.trim()
  }
  if (!metadataDraft.author.trim() && result.author.trim()) {
    metadataDraft.author = result.author.trim()
  }
  if (!metadataDraft.asset.trim() && result.level_set_name.trim()) {
    metadataDraft.asset = defaultTemplate(result.level_set_name)
  }
  if (result.info_version.trim()) metadataDraft.latestInfoVersion = result.info_version.trim()
  if (Object.keys(parsePairs(metadataDraft.requires)).length === 0 && Object.keys(result.requires).length > 0) {
    metadataDraft.requires = pairsToText(result.requires)
  }
}

/** 拖入 / 选择本地 ZIP：只识别并回填，不上传（上传是第二步的按钮）。 */
async function loadPendingZip(path: string) {
  pending.path = path
  pending.inspection = null
  const result = await inspectZip(path)
  if (!result) {
    pending.path = ''
    return
  }
  pending.inspection = result
  // ZIP 名里的版本 = 这一版的版本号（写进附件名与 latest_mapset_version）。
  pending.version = result.mapset_version.trim() || versionFromAssetName(result.file_name)
  applyInspection(result)
  if (pending.version.trim()) metadataDraft.latestMapsetVersion = pending.version.trim()
}

async function pickPendingZip() {
  try {
    const path = await api.chooseLevelSetZip()
    if (path) await loadPendingZip(path)
  } catch (reason) {
    message.error(backendMessageOf(reason))
  }
}

/**
 * 第一步：新建模式下创建 Release（不带块），地图管理下只改标题 / 正文 / 状态。
 *
 * 新建时先查一遍同名 tag：GitHub 允许在已有 tag 上建 Release，但那样会把
 * 另一张图的附件挂到同一个身份下，消费端会当成同一张图的两版。
 */
async function runReleaseStep(): Promise<boolean> {
  if (!selectedRepository.value) {
    message.warning(t('mapPublish.noSelection'))
    return false
  }
  if (stepBusy.value !== null) return false
  const tag = form.tag.trim()
  if (!tag) {
    message.warning(t('mapPublish.tagHint'))
    return false
  }
  const release = workingRelease.value
  if (!release && releases.value.some((item) => item.tag === tag)) {
    message.warning(t('mapPublish.tagExists', { tag }))
    return false
  }
  stepBusy.value = 'release'
  try {
    const saved = await saveRelease({
      releaseId: release?.id ?? null,
      tag,
      title: form.title.trim(),
      body: form.body,
      draft: form.draft,
      prerelease: form.prerelease,
    })
    if (!saved) {
      lastFailedStep.value = 'release'
      return false
    }
    createdRelease.value = saved
    activeTag.value = saved.tag
    form.tag = saved.tag
    lastFailedStep.value = null
    return true
  } finally {
    stepBusy.value = null
  }
}

/** 第二步：把拖进来的 ZIP 作为这一版的附件传上去（版本号在文件名里）。 */
async function runUploadStep(): Promise<boolean> {
  const release = workingRelease.value
  if (!release) {
    message.warning(t('mapPublish.needReleaseFirst'))
    return false
  }
  const zip = pending.inspection
  if (!pending.path || !zip) {
    message.warning(t('mapPublish.needZip'))
    return false
  }
  if (stepBusy.value !== null) return false
  const version = pending.version.trim()
  if (!version) {
    message.warning(t('mapPublish.missingVersion'))
    return false
  }
  if (!isMapVersion(version)) {
    message.warning(t('mapPublish.invalidVersion', { value: version }))
    return false
  }
  const name = pendingAssetName.value
  if (!name) {
    // 模板里没有版本占位符：传上去管理器也抽不出版本，先拦住。
    message.warning(t('mapPublish.assetNeedsVersion'))
    return false
  }
  stepBusy.value = 'asset'
  try {
    const ok = await uploadAsset(release, pending.path, name)
    if (!ok) {
      lastFailedStep.value = 'asset'
      return false
    }
    metadataDraft.latestMapsetVersion = version
    if (zip.info_version.trim()) metadataDraft.latestInfoVersion = zip.info_version.trim()
    lastUploaded.value = name
    rejectPending()
    lastFailedStep.value = null
    return true
  } finally {
    stepBusy.value = null
  }
}

/** 第三步：把当前表单写回 Release note 的 `oc2-map` 块（唯一写块的地方）。 */
async function runMetadataStep(): Promise<boolean> {
  const release = workingRelease.value
  if (!release) {
    message.warning(t('mapPublish.needReleaseFirst'))
    return false
  }
  if (stepBusy.value !== null) return false
  if (!metadataDraft.levelSetName.trim()) {
    message.warning(t('mapPublish.missingLevelSetName'))
    return false
  }
  stepBusy.value = 'metadata'
  try {
    const saved = await saveRelease({
      releaseId: release.id,
      tag: release.tag,
      title: form.title.trim(),
      body: form.body,
      draft: form.draft,
      prerelease: form.prerelease,
      metadata: buildMetadata(),
    })
    if (!saved) {
      lastFailedStep.value = 'metadata'
      return false
    }
    resetForm(saved)
    lastFailedStep.value = null
    return true
  } finally {
    stepBusy.value = null
  }
}

/** 一键按顺序执行：哪一步失败就停在哪一步，改完可以只重试那一步。 */
async function runAllSteps() {
  if (busyAny.value) return
  if (missingInputs.value.length > 0) {
    message.warning(
      t('mapPublish.sequenceNeed', { steps: missingInputs.value.map(stepTitle).join(' / ') }),
    )
    return
  }
  runningAll.value = true
  let done = 0
  let failed: PublishStepId | null = null
  try {
    for (const step of PUBLISH_STEP_ORDER) {
      const runner =
        step === 'release' ? runReleaseStep : step === 'asset' ? runUploadStep : runMetadataStep
      const ok = await runner()
      if (!ok) {
        failed = step
        break
      }
      done += 1
    }
  } finally {
    runningAll.value = false
  }
  if (failed) {
    message.warning(t('mapPublish.sequenceStopped', { step: stepTitle(failed), done }))
    return
  }
  message.success(t('mapPublish.sequenceDone', { count: done }))
}

async function retryFailedStep() {
  const step = lastFailedStep.value
  if (!step) return
  if (step === 'release') await runReleaseStep()
  else if (step === 'asset') await runUploadStep()
  else await runMetadataStep()
}

/*
 * 窗口拖放由页面声明：地图管理与新建地图集两态都收 ZIP，拖进来只识别、不
 * 自动上传。列表页没有「当前地图集」，拖进来不做事（全窗提示条仍会说明只收 ZIP）。
 */
const { dragActive } = useFileDrop({
  accept: ['zip'],
  busy: () => mode.value === 'list' || busyAny.value,
  onDrop: (path) => {
    if (mode.value === 'list') return
    void loadPendingZip(path)
  },
})

/* ===== GitHub 登录 ===== */

async function openExternal(url: string) {
  try {
    const result = await api.openReleasePage(url)
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) {
    message.error(backendMessageOf(reason))
  }
}

/** 浏览器没弹出来（或被用户关掉）时，用同一个地址再打开一次。 */
function reopenAuthorization() {
  const url = authorization.value?.authorize_url
  if (url) void openExternal(url)
}

/* ===== 新建仓库 ===== */

const createOpen = ref(false)
const newRepository = reactive({ name: '', description: '', private: false })

async function submitCreateRepository() {
  const name = newRepository.name.trim()
  if (!name) {
    message.warning(t('mapPublish.repositoryName'))
    return
  }
  const created = await createRepository({
    name,
    description: newRepository.description.trim(),
    private: newRepository.private,
  })
  if (!created) return
  createOpen.value = false
  newRepository.name = ''
  newRepository.description = ''
  newRepository.private = false
}

/* ===== 删除确认 ===== */

function confirmDeleteRelease(release: PublishRelease) {
  let deleteTag = true
  const instance = dialog.warning({
    title: t('mapPublish.deleteRelease'),
    content: () =>
      h('div', { style: 'display: flex; flex-direction: column; gap: 10px;' }, [
        h('div', t('mapPublish.deleteReleaseConfirm', { tag: release.tag })),
        h(
          NCheckbox,
          {
            defaultChecked: true,
            'onUpdate:checked': (value: boolean) => { deleteTag = value },
          },
          { default: () => t('mapPublish.deleteTagToo') },
        ),
        h(
          'div',
          { style: 'font-size: 12px; line-height: 1.6; color: var(--oc2-text-3);' },
          t('mapPublish.deleteTagHint'),
        ),
      ]),
    positiveText: t('common.action.delete'),
    negativeText: t('common.action.cancel'),
    onPositiveClick: async () => {
      markDialogBusy(instance)
      const ok = await deleteRelease(release, deleteTag)
      if (ok && activeTag.value === release.tag) backToList()
    },
  })
}

function confirmDeleteAsset(asset: PublishAsset) {
  const instance = dialog.warning({
    title: t('mapPublish.deleteAsset'),
    content: t('mapPublish.deleteAssetConfirm', { name: asset.name }),
    positiveText: t('common.action.delete'),
    negativeText: t('common.action.cancel'),
    onPositiveClick: async () => {
      markDialogBusy(instance)
      await deleteAsset(asset.id)
    },
  })
}
</script>

<template>
  <Teleport to="#page-header-actions" defer>
    <n-button size="small" :loading="repositoriesLoading || releasesLoading" :disabled="!authenticated"
      @click="refreshRepositories">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('common.action.refresh') }}
    </n-button>

    <!--
      GitHub 账号与仓库范围都在这里，主区只留「当前仓库的地图集」。
      面板里不用 n-select：它的下拉是 teleport 到 body 的，点选项会被 popover
      当成外面点了一下而立刻收起。仓库列表自己渲染，搜索框也只筛本地数组。
    -->
    <n-popover v-model:show="settingsOpen" trigger="click" placement="bottom-end" :show-arrow="false" raw
      :z-index="2400">
      <template #trigger>
        <n-button size="small" :title="t('mapPublish.settings')" :aria-label="t('mapPublish.settings')">
          <template #icon><SettingsOutline width="14" height="14" /></template>
          {{ t('mapPublish.settings') }}
        </n-button>
      </template>

      <div class="publish-settings">
        <h3 class="settings-title">{{ t('mapPublish.account') }}</h3>

        <div v-if="!authReady" class="state-line">
          <n-spin size="small" />
          <span>{{ t('common.loading') }}</span>
        </div>

        <template v-else-if="authenticated">
          <div class="signed-in">
            <span class="signed-in-text">{{ t('mapPublish.signedInAs', { login: auth?.login ?? '' }) }}</span>
            <n-button size="tiny" :loading="signingOut" :disabled="signingOut" @click="signOut">
              {{ t('mapPublish.signOut') }}
            </n-button>
          </div>
          <p class="hint no-top">{{ t('mapPublish.installAppHint') }}</p>
          <div class="code-actions">
            <n-button size="tiny" @click="openInstallPage">{{ t('mapPublish.installApp') }}</n-button>
          </div>
        </template>

        <div v-else-if="authorization" class="auth-block">
          <p class="hint no-top">{{ t('mapPublish.waiting') }}</p>
          <p v-if="authorization.mode === 'install'" class="hint no-top">
            {{ t('mapPublish.installFallbackHint') }}
          </p>
          <code class="auth-url mono">{{ authorization.authorize_url }}</code>
          <div class="code-actions">
            <n-button size="tiny" @click="reopenAuthorization">{{ t('mapPublish.reopenBrowser') }}</n-button>
            <n-button v-if="authorization.mode === 'install'" size="tiny" @click="startSignInAnyway">
              {{ t('mapPublish.signInAnyway') }}
            </n-button>
            <n-button size="tiny" @click="cancelSignIn">{{ t('mapPublish.cancelSignIn') }}</n-button>
          </div>
        </div>

        <div v-else class="auth-block">
          <p class="hint no-top">{{ t('mapPublish.signInHint') }}</p>
          <p v-if="auth && !auth.secret_configured" class="warn no-top">{{ t('mapPublish.secretMissing') }}</p>
          <p v-else-if="authPhase === 'expired'" class="warn no-top">{{ t('mapPublish.expired') }}</p>
          <p v-else-if="authPhase === 'denied'" class="warn no-top">{{ t('mapPublish.denied') }}</p>
          <p v-else-if="authPhase === 'failed'" class="warn no-top">{{ authFailure }}</p>
          <div class="code-actions">
            <n-button size="tiny" type="primary" :loading="signingIn"
              :disabled="signingIn || (auth !== null && !auth.secret_configured)" @click="startSignIn">
              {{ t('mapPublish.signIn') }}
            </n-button>
          </div>
        </div>

        <template v-if="authenticated">
          <div class="settings-divider" />
          <h3 class="settings-title">{{ t('mapPublish.repositories') }}</h3>

          <div v-if="needsInstall" class="auth-block">
            <p class="hint no-top">{{ t('mapPublish.installStepHint') }}</p>
            <p class="hint no-top">{{ t('mapPublish.repositoryEmptyInstall') }}</p>
            <div class="code-actions">
              <n-button size="tiny" type="primary" :loading="signingIn" :disabled="signingIn"
                @click="startSignIn">
                {{ t('mapPublish.installAppNow') }}
              </n-button>
            </div>
          </div>

          <template v-else>
            <div class="repo-tools">
              <n-input v-model:value="repositoryFilter" size="tiny" clearable
                :placeholder="t('mapPublish.searchRepository')" />
              <n-button size="tiny" :disabled="creatingRepository" @click="createOpen = !createOpen">
                {{ t('mapPublish.createRepository') }}
              </n-button>
            </div>

            <div v-if="repositoriesLoading" class="state-line">
              <n-spin size="small" />
              <span>{{ t('common.loading') }}</span>
            </div>
            <ul v-else-if="filteredRepositories.length" class="repo-list">
              <li v-for="repository in filteredRepositories" :key="repository.full_name">
                <button type="button" class="repo-item"
                  :class="{ active: repository.full_name === selectedFullName }"
                  @click="chooseRepository(repository.full_name)">
                  <span class="repo-name mono">{{ repository.full_name }}</span>
                  <n-tag v-if="repository.topics.includes('map-source')" size="tiny" type="success" :bordered="false">
                    map-source
                  </n-tag>
                </button>
              </li>
            </ul>
            <p v-else-if="repositories.length" class="hint no-top">{{ t('mapPublish.searchRepositoryEmpty') }}</p>
            <p v-else class="hint no-top">{{ t('mapPublish.repositoryEmpty') }}</p>

            <p class="hint no-top">{{ t('mapPublish.repositoryHint') }}</p>
            <p v-if="!allRepositories" class="hint no-top">{{ t('mapPublish.createRepositoryLimited') }}</p>

            <div v-if="createOpen" class="create-form">
              <p class="hint no-top">{{ t('mapPublish.createRepositoryHint') }}</p>
              <div class="form-grid">
                <div class="field">
                  <span class="field-label">{{ t('mapPublish.repositoryName') }}</span>
                  <n-input v-model:value="newRepository.name" size="small" />
                </div>
                <div class="field">
                  <span class="field-label">{{ t('mapPublish.repositoryDescription') }}</span>
                  <n-input v-model:value="newRepository.description" size="small" />
                </div>
                <div class="field span-2 switch-field">
                  <div>
                    <span class="field-label">{{ t('mapPublish.repositoryPrivate') }}</span>
                    <p class="hint no-top">{{ t('mapPublish.repositoryPrivateHint') }}</p>
                  </div>
                  <n-switch v-model:value="newRepository.private" size="small" />
                </div>
              </div>
              <div class="actions">
                <n-button size="tiny" type="primary" :loading="creatingRepository" :disabled="creatingRepository"
                  @click="submitCreateRepository">
                  {{ t('mapPublish.create') }}
                </n-button>
                <n-button size="tiny" :disabled="creatingRepository" @click="createOpen = false">
                  {{ t('common.action.cancel') }}
                </n-button>
              </div>
            </div>
          </template>
        </template>
      </div>
    </n-popover>
  </Teleport>

  <PageLoading :show="authLoading">
    <template #status>{{ t('common.loading') }}</template>

    <p class="page-intro muted">{{ t('mapPublish.subtitle') }}</p>

    <!-- 未登录：主区只留入口；没装过 App 时按钮直接打开安装页，装与选仓库一步完成 -->
    <section v-if="!authenticated" class="group">
      <h2 class="group-title">
        {{ auth?.installed ? t('mapPublish.signIn') : t('mapPublish.installAppNow') }}
      </h2>
      <div class="panel">
        <div v-if="!authReady" class="state-line">
          <n-spin size="small" />
          <span>{{ t('common.loading') }}</span>
        </div>
        <template v-else>
          <p class="hint no-top">
            {{ auth?.installed ? t('mapPublish.connectRequired') : t('mapPublish.installStepHint') }}
          </p>
          <p v-if="auth && !auth.secret_configured" class="warn no-top">{{ t('mapPublish.secretMissing') }}</p>
          <p v-else-if="authPhase === 'expired'" class="warn no-top">{{ t('mapPublish.expired') }}</p>
          <p v-else-if="authPhase === 'denied'" class="warn no-top">{{ t('mapPublish.denied') }}</p>
          <p v-else-if="authPhase === 'failed'" class="warn no-top">{{ authFailure }}</p>
          <p v-if="authorization" class="hint">{{ t('mapPublish.waiting') }}</p>
          <p class="hint no-top">
            {{ t('mapPublish.installFallbackHint') }}
          </p>
          <div class="actions">
            <n-button size="small" type="primary" :loading="signingIn"
              :disabled="signingIn || (auth !== null && !auth.secret_configured)" @click="startSignIn">
              {{ auth?.installed ? t('mapPublish.signIn') : t('mapPublish.installAppNow') }}
            </n-button>
            <n-button size="small" @click="startSignInAnyway">
              {{ t('mapPublish.signInAnyway') }}
            </n-button>
            <n-button v-if="authorization" size="small" @click="reopenAuthorization">
              {{ t('mapPublish.reopenBrowser') }}
            </n-button>
            <n-button v-if="authorization" size="small" @click="cancelSignIn">
              {{ t('mapPublish.cancelSignIn') }}
            </n-button>
          </div>
        </template>
      </div>
    </section>

    <!-- 已登录但还没定下仓库：安装 App 与选仓库都在设置面板里，这里只引导一次 -->
    <section v-else-if="!selectedRepository" class="group">
      <h2 class="group-title">{{ t('mapPublish.repositories') }}</h2>
      <div class="panel">
        <div v-if="repositoriesLoading" class="state-line">
          <n-spin size="small" />
          <span>{{ t('common.loading') }}</span>
        </div>
        <template v-else>
          <p v-if="needsInstall" class="hint no-top">{{ t('mapPublish.installStepHint') }}</p>
          <p v-else-if="!repositories.length" class="hint no-top">{{ t('mapPublish.repositoryEmpty') }}</p>
          <p v-else class="hint no-top">{{ t('mapPublish.chooseRepositoryHint') }}</p>
          <p v-if="authorization" class="hint no-top">{{ t('mapPublish.waiting') }}</p>
          <p class="hint no-top">
            {{ t('mapPublish.installFallbackHint') }}
          </p>
          <div class="actions">
            <n-button v-if="needsInstall" size="small" type="primary" :loading="signingIn"
              :disabled="signingIn" @click="startSignIn">
              {{ t('mapPublish.installAppNow') }}
            </n-button>
            <n-button size="small" @click="startSignInAnyway">
              {{ t('mapPublish.signInAnyway') }}
            </n-button>
            <n-button v-if="!needsInstall" size="small" type="primary" @click="openSettings">
              {{ t('mapPublish.selectRepository') }}
            </n-button>
            <n-button v-if="authorization" size="small" @click="reopenAuthorization">
              {{ t('mapPublish.reopenBrowser') }}
            </n-button>
            <n-button v-if="authorization" size="small" @click="cancelSignIn">
              {{ t('mapPublish.cancelSignIn') }}
            </n-button>
          </div>
        </template>
      </div>
    </section>

    <!-- 仓库已定：主区在地图集列表 / 地图管理 / 新建地图集之间切换 -->
    <section v-else class="group">
      <h2 class="group-title">
        {{ mode === 'list' ? t('mapPublish.mapSets') : t('mapPublish.mapManagement') }}
      </h2>

      <!-- 选了仓库就查一次标签：缺 map-source 之类，地图页就收不到这个仓库 -->
      <div v-if="missingTopics.length" class="panel topics-banner">
        <div class="topics-text">
          <span class="topics-title">{{ t('mapPublish.topicsMissingTitle') }}</span>
          <p class="hint no-top">{{ t('mapPublish.topicsMissingHint', { topics: missingTopics.join(', ') }) }}</p>
          <p class="hint no-top">{{ t('mapPublish.topicsFixHint') }}</p>
        </div>
      </div>

      <!-- 地图集列表：一个 tag 一张卡 -->
      <div v-if="mode === 'list'" class="panel">
        <div class="panel-head">
          <span class="muted mono">{{ selectedRepository?.full_name ?? '' }}</span>
          <n-button size="small" type="primary" :disabled="releasesLoading" @click="openCreate">
            {{ t('mapPublish.newMapSet') }}
          </n-button>
        </div>
        <p class="hint no-top">{{ t('mapPublish.repositoryScopeHint') }}</p>

        <div v-if="releasesLoading" class="state-line">
          <n-spin size="small" />
          <span>{{ t('common.loading') }}</span>
        </div>
        <p v-else-if="!releases.length" class="hint no-top">{{ t('mapPublish.mapSetEmpty') }}</p>
        <ul v-else class="release-list">
          <li v-for="release in releases" :key="release.id" class="release-row">
            <div class="release-main">
              <div class="release-head">
                <code class="release-tag mono">{{ release.tag }}</code>
                <span class="release-title">{{ displayName(release) }}</span>
                <n-tag v-if="release.metadata?.level_set_name_zh" size="tiny" :bordered="false">
                  {{ release.metadata.level_set_name_zh }}
                </n-tag>
                <n-tag v-if="release.metadata?.author" size="tiny" :bordered="false">
                  {{ release.metadata.author }}
                </n-tag>
                <n-tag v-if="release.draft" size="tiny" :bordered="false">{{ t('mapPublish.draft') }}</n-tag>
                <n-tag v-if="release.prerelease" size="tiny" type="warning" :bordered="false">
                  {{ t('mapPublish.prerelease') }}
                </n-tag>
              </div>
              <!-- 没有 oc2-map 块 = 这个 Release 还不是地图，进管理页保存后才会补上 -->
              <small v-if="!release.metadata" class="muted">{{ t('mapPublish.noMetadata') }}</small>
              <small v-else class="muted">
                {{ t('mapPublish.latestVersion') }}：
                <code class="mono">{{ latestVersionOfRelease(release) || '—' }}</code>
                · {{ release.assets.length }} {{ t('mapPublish.assets') }}
              </small>
            </div>
            <div class="release-actions">
              <n-button size="small" @click="openManagement(release)">
                {{ t('mapPublish.mapManagement') }}
              </n-button>
              <n-button size="small" type="error" secondary :disabled="deletingRelease"
                @click="confirmDeleteRelease(release)">
                {{ t('mapPublish.deleteRelease') }}
              </n-button>
            </div>
          </li>
        </ul>
      </div>

      <template v-else>
        <!-- 一张地图集 = 三步：发布信息 → 上传 ZIP → 写回地图信息；也能一键跑完 -->
        <div class="panel publish-head">
          <div class="panel-head">
            <div class="manage-head">
              <code class="release-tag mono">
                {{ workingRelease?.tag || form.tag.trim() || t('mapPublish.newMapSet') }}
              </code>
              <span class="release-title">
                {{ workingRelease ? displayName(workingRelease) : form.title.trim() }}
              </span>
              <n-tag size="tiny" :bordered="false">
                {{ workingRelease ? t('mapPublish.mapManagement') : t('mapPublish.newMapSet') }}
              </n-tag>
            </div>
            <n-button size="small" :disabled="busyAny" @click="backToList">
              {{ t('mapPublish.backToMapSets') }}
            </n-button>
          </div>
          <p class="hint no-top">{{ t('mapPublish.flowHint') }}</p>
        </div>

        <!-- 缺权限 / 被限流的常驻引导：写失败后出现，写成功一次自动收起 -->
        <div v-if="permissionFailure" class="panel permission-banner">
          <div class="topics-text">
            <span class="topics-title">{{ t('mapPublish.permissionTitle') }}</span>
            <p v-if="permissionFailure.rateLimited" class="hint no-top">
              {{ t('mapPublish.rateLimitedHint', { reset: permissionResetText }) }}
            </p>
            <template v-else>
              <p class="hint no-top">
                {{ t('mapPublish.permissionHint', { permission: permissionLabel }) }}
              </p>
              <ol class="permission-steps">
                <li>{{ t('mapPublish.permissionStep1') }}</li>
                <li>{{ t('mapPublish.permissionStep2') }}</li>
                <li>{{ t('mapPublish.permissionStep3') }}</li>
              </ol>
            </template>
          </div>
          <div class="banner-actions">
            <n-button size="small" @click="openInstallPage">{{ t('mapPublish.manageAppInstall') }}</n-button>
            <n-button size="small" type="primary" :loading="signingIn" :disabled="signingIn"
              @click="startSignInAnyway">
              {{ t('mapPublish.reauthorize') }}
            </n-button>
            <n-button v-if="lastFailedStep && !permissionFailure.rateLimited" size="small"
              :disabled="busyAny" @click="retryFailedStep">
              {{ t('mapPublish.retryStep') }}
            </n-button>
          </div>
        </div>

        <!-- 步骤 1：Release 基本信息（tag / 标题 / 说明正文） -->
        <div class="panel step-panel">
          <div class="step-head">
            <span class="step-index">1</span>
            <h3 class="sub-title">{{ t('mapPublish.stepReleaseTitle') }}</h3>
            <n-tag size="tiny" :type="stepReleaseDone ? 'success' : 'default'" :bordered="false">
              {{ stepReleaseDone ? t('mapPublish.stepDone') : t('mapPublish.stepPending') }}
            </n-tag>
          </div>
          <p class="hint no-top">{{ t('mapPublish.stepReleaseHint') }}</p>

          <div class="form-grid">
            <div class="field">
              <span class="field-label">{{ t('mapPublish.tagLabel') }}</span>
              <n-input v-model:value="form.tag" size="small" placeholder="chan"
                :disabled="stepReleaseDone || busyAny" />
              <p class="hint">{{ t('mapPublish.tagHint') }}</p>
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.titleLabel') }}</span>
              <n-input v-model:value="form.title" size="small" :disabled="busyAny" />
            </div>
            <div class="field span-2">
              <span class="field-label">{{ t('mapPublish.bodyLabel') }}</span>
              <n-input v-model:value="form.body" type="textarea" :autosize="{ minRows: 4, maxRows: 12 }"
                :disabled="busyAny" />
              <p class="hint">{{ t('mapPublish.bodyHint') }}</p>
            </div>
            <div class="field span-2 switch-field">
              <label class="switch-label">
                <n-switch v-model:value="form.draft" size="small" :disabled="busyAny" />
                <span>{{ t('mapPublish.draft') }}</span>
              </label>
              <label class="switch-label">
                <n-switch v-model:value="form.prerelease" size="small" :disabled="busyAny" />
                <span>{{ t('mapPublish.prerelease') }}</span>
              </label>
            </div>
          </div>

          <div class="actions">
            <n-button size="small" type="primary" :loading="stepBusy === 'release'"
              :disabled="busyAny || !form.tag.trim()" @click="runReleaseStep">
              {{ stepReleaseDone ? t('mapPublish.saveReleaseInfo') : t('mapPublish.createRelease') }}
            </n-button>
          </div>
        </div>

        <!-- 步骤 2：上传这一版的 ZIP（拖进来先识别，改完版本号再上传） -->
        <div class="panel step-panel">
          <div class="step-head">
            <span class="step-index">2</span>
            <h3 class="sub-title">{{ t('mapPublish.stepAssetTitle') }}</h3>
            <n-tag size="tiny" :type="stepAssetDone ? 'success' : 'default'" :bordered="false">
              {{ stepAssetDone ? t('mapPublish.stepDone') : t('mapPublish.stepPending') }}
            </n-tag>
          </div>
          <p class="hint no-top">{{ t('mapPublish.stepAssetHint') }}</p>

          <div class="drop-zone" :class="{ active: dragActive }" role="button" tabindex="0"
            @click="pickPendingZip" @keydown.enter="pickPendingZip">
            <p class="drop-title">{{ t('mapPublish.dropZone') }}</p>
            <p class="hint no-top">{{ t('mapPublish.addAssetHint') }}</p>
            <p v-if="pendingZipName" class="mono picked-zip">{{ pendingZipName }}</p>
          </div>

          <div v-if="inspecting" class="state-line">
            <n-spin size="small" />
            <span>{{ t('mapPublish.inspectingZip') }}</span>
          </div>

          <template v-if="pending.inspection">
            <div class="inspection-grid">
              <div>
                <span class="field-label">{{ t('mapPublish.inspectionMapsetVersion') }}</span>
                <span class="mono">{{ pending.inspection.mapset_version || '—' }}</span>
              </div>
              <div>
                <span class="field-label">{{ t('mapPublish.inspectionInfoVersion') }}</span>
                <span class="mono">{{ pending.inspection.info_version || '—' }}</span>
              </div>
              <div>
                <span class="field-label">{{ t('mapPublish.inspectionLevelSet') }}</span>
                <span>
                  {{ pending.inspection.level_set_name || '—' }}
                  <template v-if="pending.inspection.level_set_name_zh">
                    · {{ pending.inspection.level_set_name_zh }}
                  </template>
                  <template v-if="pending.inspection.author"> · {{ pending.inspection.author }}</template>
                </span>
              </div>
              <div v-if="Object.keys(pending.inspection.requires).length">
                <span class="field-label">{{ t('mapPublish.inspectionRequires') }}</span>
                <span class="mono">{{ requiresText(pending.inspection.requires) }}</span>
              </div>
            </div>
            <p v-if="pending.inspection.set_count > 1" class="warn no-top">
              {{ t('mapPublish.inspectionMultiple', { count: pending.inspection.set_count }) }}
            </p>

            <div class="form-grid">
              <div class="field">
                <span class="field-label">{{ t('mapPublish.releaseVersionLabel') }}</span>
                <n-input v-model:value="pending.version" size="small" placeholder="1.2.0"
                  :disabled="busyAny" />
                <p class="hint">{{ t('mapPublish.releaseVersionHint') }}</p>
                <p v-if="!pending.version.trim()" class="warn no-top">{{ t('mapPublish.versionFromNameMissing') }}</p>
              </div>
              <div class="field">
                <span class="field-label">{{ t('mapPublish.assetExpected', { name: pendingAssetName || '—' }) }}</span>
                <p v-if="!pendingAssetName" class="warn no-top">{{ t('mapPublish.assetNeedsVersion') }}</p>
                <p v-else class="hint no-top">{{ t('mapPublish.uploadHint') }}</p>
              </div>
            </div>
          </template>

          <div class="actions">
            <n-button size="small" type="primary" :loading="stepBusy === 'asset'"
              :disabled="busyAny || !stepReleaseDone || !pendingAssetName" @click="runUploadStep">
              {{ t('mapPublish.uploadZip') }}
            </n-button>
            <span v-if="lastUploaded" class="muted uploaded-note">
              {{ t('mapPublish.uploadedAs', { name: lastUploaded }) }}
            </span>
          </div>
        </div>

        <!-- 步骤 3：写回 oc2-map 块 —— 只有这一步会让地图在管理器里可见 -->
        <div class="panel step-panel">
          <div class="step-head">
            <span class="step-index">3</span>
            <h3 class="sub-title">{{ t('mapPublish.stepMetadataTitle') }}</h3>
            <n-tag size="tiny" :type="stepMetadataDone ? 'success' : 'default'" :bordered="false">
              {{ stepMetadataDone ? t('mapPublish.stepDone') : t('mapPublish.stepPending') }}
            </n-tag>
          </div>
          <p class="hint no-top">{{ t('mapPublish.stepMetadataHint') }}</p>

          <div class="form-grid">
            <div class="field">
              <span class="field-label">{{ t('mapPublish.levelSetName') }}</span>
              <n-input v-model:value="metadataDraft.levelSetName" size="small" :disabled="busyAny" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.levelSetNameZh') }}</span>
              <n-input v-model:value="metadataDraft.levelSetNameZh" size="small" :disabled="busyAny" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.author') }}</span>
              <n-input v-model:value="metadataDraft.author" size="small" :disabled="busyAny" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.assetTemplate') }}</span>
              <n-input v-model:value="metadataDraft.asset" size="small" placeholder="gaoda-${version}.zip"
                :disabled="busyAny" />
              <p class="hint">{{ t('mapPublish.assetHint') }}</p>
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.latestMapsetVersion') }}</span>
              <n-input v-model:value="metadataDraft.latestMapsetVersion" size="small" :disabled="busyAny" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.latestInfoVersion') }}</span>
              <n-input v-model:value="metadataDraft.latestInfoVersion" size="small" :disabled="busyAny" />
            </div>
            <div class="field span-2">
              <p class="hint no-top">{{ t('mapPublish.versionHint') }}</p>
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.nameI18n') }}</span>
              <n-input v-model:value="metadataDraft.nameI18n" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="busyAny" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.groupI18n') }}</span>
              <n-input v-model:value="metadataDraft.groupI18n" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="busyAny" />
            </div>
            <div class="field span-2">
              <span class="field-label">{{ t('mapPublish.noteI18n') }}</span>
              <n-input v-model:value="metadataDraft.noteI18n" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="busyAny" />
              <p class="hint">{{ t('mapPublish.i18nHint') }}</p>
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.requires') }}</span>
              <n-input v-model:value="metadataDraft.requires" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="busyAny" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.compatible') }}</span>
              <n-input v-model:value="metadataDraft.compatible" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="busyAny" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.conflicts') }}</span>
              <n-input v-model:value="metadataDraft.conflicts" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="busyAny" />
            </div>
            <div class="field">
              <p class="hint align-end">{{ t('mapPublish.relationHint') }}</p>
            </div>
          </div>

          <div class="actions">
            <n-button size="small" type="primary" :loading="stepBusy === 'metadata'"
              :disabled="busyAny || !stepReleaseDone || !metadataDraft.levelSetName.trim()"
              @click="runMetadataStep">
              {{ t('mapPublish.saveMetadata') }}
            </n-button>
          </div>
        </div>

        <!-- 一键按顺序执行：三步都就绪时才建议用 -->
        <div class="panel sequence-panel">
          <div class="topics-text">
            <span class="topics-title">{{ t('mapPublish.sequenceTitle') }}</span>
            <p class="hint no-top">{{ sequenceHint }}</p>
          </div>
          <n-button size="small" type="primary" :loading="runningAll" :disabled="busyAny"
            @click="runAllSteps">
            {{ t('mapPublish.runAll') }}
          </n-button>
        </div>

        <!-- 已有资产：一个附件 = 一个版本，版本号在文件名里 -->
        <div v-if="workingRelease" class="panel assets-panel">
          <div class="panel-head">
            <h3 class="sub-title">{{ t('mapPublish.assets') }}</h3>
            <span class="muted">{{ workingRelease.assets.length }}</span>
          </div>
          <p class="hint no-top">{{ t('mapPublish.assetsHint') }}</p>
          <p v-if="!workingRelease.assets.length" class="hint">{{ t('mapPublish.assetEmpty') }}</p>
          <ul v-else class="asset-list">
            <li v-for="asset in workingRelease.assets" :key="asset.id" class="asset-row">
              <code class="mono">{{ asset.name }}</code>
              <n-tag v-if="versionFromAssetName(asset.name)" size="tiny" :bordered="false">
                {{ versionFromAssetName(asset.name) }}
              </n-tag>
              <span class="muted">{{ formatFileSize(asset.size) }}</span>
              <n-button size="tiny" type="error" secondary :disabled="deletingAsset"
                @click="confirmDeleteAsset(asset)">
                {{ t('mapPublish.deleteAsset') }}
              </n-button>
            </li>
          </ul>
        </div>
      </template>
    </section>
  </PageLoading>


</template>

<style scoped>
.page-intro {
  margin: 0 0 18px;
  font-size: 13px;
  line-height: 1.6;
}

.group { margin-bottom: 22px; }

.group-title {
  margin: 0 0 10px;
  color: var(--oc2-accent);
  font-size: 13px;
  font-weight: 700;
  letter-spacing: .04em;
}

.panel {
  padding: 16px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-row);
}
.assets-panel { margin-top: 16px; }

.topics-banner {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 16px;
  border-color: var(--oc2-accent);
}
.topics-text { min-width: 0; }
.topics-title { font-size: 13px; font-weight: 700; }

.hint {
  margin: 8px 0 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--oc2-text-3);
}
.hint.no-top { margin-top: 0; }
.hint.align-end { margin: 0; align-self: end; padding-bottom: 6px; }
.warn {
  margin: 10px 0 0;
  font-size: 12px;
  line-height: 1.6;
  color: var(--oc2-accent);
}
.warn.no-top { margin-top: 0; }

.state-line {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 12px;
  font-size: 13px;
  color: var(--oc2-text-2);
}

.signed-in {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
}
.signed-in-text { font-size: 13.5px; font-weight: 600; }

.auth-block { display: flex; flex-direction: column; gap: 10px; margin-top: 10px; }
.auth-url {
  margin: 0;
  font-size: 12px;
  color: var(--oc2-text-3);
  overflow-wrap: anywhere;
}
.code-actions { display: flex; gap: 10px; }

/* 页头设置面板：popover 的 raw 内容，样式跟着组件作用域走（teleport 只搬 DOM 不搬 scope id） */
.publish-settings {
  display: flex;
  flex-direction: column;
  gap: 10px;
  width: 340px;
  max-height: min(72vh, 600px);
  overflow-y: auto;
  padding: 14px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-popover);
  box-shadow: 0 16px 44px rgba(0, 0, 0, .42);
}
.settings-title {
  margin: 0;
  color: var(--oc2-accent);
  font-size: 12px;
  font-weight: 700;
  letter-spacing: .04em;
}
.settings-divider { height: 1px; margin: 2px 0; background: var(--oc2-divider); }

.repo-tools { display: flex; align-items: center; gap: 8px; }
.repo-tools .n-input { flex: 1 1 auto; min-width: 0; }

.repo-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  max-height: 220px;
  overflow-y: auto;
}
.repo-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 6px 8px;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  color: var(--oc2-text-2);
  font: inherit;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}
.repo-item:hover { background: var(--oc2-tile); }
.repo-item.active {
  border-color: var(--oc2-accent);
  background: var(--oc2-tile);
  color: var(--oc2-text-1);
}
.repo-name {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 面板比页面窄：新建仓库表单收成一列，按钮行也收紧 */
.publish-settings .form-grid { grid-template-columns: minmax(0, 1fr); gap: 10px; }
.publish-settings .actions { margin-top: 10px; }
.publish-settings .create-form { margin-top: 10px; padding-top: 10px; }

.create-form {
  margin-top: 14px;
  padding-top: 14px;
  border-top: 1px solid var(--oc2-divider);
}

.form-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 14px;
  margin-top: 14px;
}
.form-grid .field { display: flex; flex-direction: column; gap: 6px; min-width: 0; }
.span-2 { grid-column: 1 / -1; }
.field-label { font-size: 12px; font-weight: 600; color: var(--oc2-text-2); }
.switch-field { flex-direction: row !important; align-items: center; gap: 18px !important; }
.switch-label {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--oc2-text-2);
  cursor: pointer;
}

.section-divider {
  height: 1px;
  margin: 18px 0 14px;
  background: var(--oc2-divider);
}

.sub-title { margin: 0 0 4px; font-size: 13.5px; font-weight: 650; }

.actions { display: flex; gap: 10px; margin-top: 18px; }

.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-bottom: 12px;
}
.manage-head { display: flex; align-items: center; gap: 10px; min-width: 0; }

.release-list,
.asset-list {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.release-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  background: var(--oc2-tile);
}
.release-main { display: flex; flex-direction: column; gap: 4px; min-width: 0; }
.release-head { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; min-width: 0; }
.release-tag { font-size: 12px; color: var(--oc2-text-2); }
.release-title {
  font-size: 13px;
  font-weight: 600;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 420px;
}
.release-actions { display: flex; gap: 8px; flex: 0 0 auto; }

.asset-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 12px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  background: var(--oc2-tile);
}
.asset-row code {
  flex: 1 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 12px;
}
.asset-row .muted { flex: 0 0 auto; font-size: 12px; }

.picked-zip {
  font-size: 12px;
  max-width: 360px;
  margin: 8px 0 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 拖放区：弹窗里唯一入口，拖悬停时整块亮起 */
.drop-zone {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  padding: 22px 16px;
  border: 1px dashed var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-tile);
  text-align: center;
  cursor: pointer;
  transition: border-color .15s ease, background .15s ease;
}
.drop-zone:hover { border-color: var(--oc2-accent); }
.drop-zone.active {
  border-color: var(--oc2-accent);
  background: var(--oc2-row);
}
.drop-title { margin: 0; font-size: 13px; font-weight: 600; color: var(--oc2-text-1); }

/* 识别结果：左标签右值，值可换行 */
.inspection-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 10px 16px;
  margin-top: 10px;
}
.inspection-grid > div { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.inspection-grid span { font-size: 12px; color: var(--oc2-text-2); overflow-wrap: anywhere; }

/* ===== 三步式发布 ===== */

.publish-head { margin-bottom: 16px; }
.step-panel { margin-top: 16px; }
.step-head {
  display: flex;
  align-items: center;
  gap: 8px;
}
.step-head .sub-title { margin: 0; }
.step-index {
  display: inline-flex;
  flex: 0 0 auto;
  align-items: center;
  justify-content: center;
  width: 20px;
  height: 20px;
  border-radius: 50%;
  background: var(--oc2-accent);
  color: var(--oc2-body);
  font-size: 12px;
  font-weight: 700;
}

.sequence-panel {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  margin-top: 16px;
}

.permission-banner {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 16px;
  border-color: var(--oc2-accent);
}
.permission-banner .banner-actions {
  display: flex;
  flex: 0 0 auto;
  flex-direction: column;
  gap: 8px;
}
.permission-steps {
  margin: 8px 0 0;
  padding-left: 18px;
  font-size: 12px;
  line-height: 1.7;
  color: var(--oc2-text-3);
}

.uploaded-note {
  align-self: center;
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
