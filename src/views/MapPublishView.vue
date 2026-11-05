<script setup lang="ts">
import { computed, h, nextTick, onMounted, reactive, ref, watch } from 'vue'
import {
  NButton,
  NCheckbox,
  NCollapse,
  NCollapseItem,
  NInput,
  NPopover,
  NSpin,
  NSwitch,
  NTag,
  useDialog,
} from 'naive-ui'
import { RefreshOutline, SettingsOutline, TrashOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { api } from '../services/tauri'
import PageLoading from '../components/PageLoading.vue'
import VersionOrderTable from '../components/publish/VersionOrderTable.vue'
import { useFileDrop } from '../composables/useFileDrop'
import { useMapPublish } from '../composables/useMapPublish'
import {
  assetRuleFrom,
  compareMapVersions,
  composeAssetRule,
  isVersionText,
  latestVersionOf,
  parseVersionedZipName,
  renderAssetName,
  versionFromAssetName,
  versionFromRule,
} from '../mapPublishAsset'
import {
  PUBLISH_STEP_ORDER,
  defaultMapTag,
  missingPublishInputs,
  permissionScopeKey,
  type AssetVersionRow,
  type PublishStepId,
} from '../mapPublishFlow'
import {
  I18N_FIELD_ORDER,
  MAP_I18N_LANGUAGES,
  MAP_I18N_LANGUAGE_LABELS,
  blankI18nField,
  defaultI18nValues,
  extraLanguages,
  followDefaults,
  i18nDraftFrom,
  i18nTableOf,
  missingRequiredLanguages,
  type I18nDefaults,
  type I18nFieldDraft,
  type I18nFieldId,
} from '../mapPublishI18n'
import { markDialogBusy } from '../dialogBusy'
import { useNotify } from '../useNotify'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import type { MapPublishMetadata, PublishAsset, PublishRelease, PublishZipInspection } from '../types'


/**
 * 地图发布页 —— 作者把地图 ZIP 作为 GitHub Release 发出去的唯一入口。
 *
 * 口径：**一个 tag 就是一张地图集**，版本号只写在附件名里。进到某张地图集里
 * 只有两个动作：
 *
 * 1. **发布这一版**：拖入 ZIP → 建/改 Release → 传附件 → 写 `oc2-map` 块，
 *    一次点按走完（`publishVersion`），三段进度就地显示，失败可单独重试。
 * 2. **保存地图资料**：基本信息、多语言、版本顺序、关系表、GitHub 页面，
 *    一个保存按钮（`saveDocument`）—— 它是**唯一**的写块入口。
 *
 * 为什么这样分：块里的版本表与正文是同一份文档，两个按钮各写一半会互相覆盖；
 * 以前「修改信息」发的是不带 metadata 的 PATCH，正文里的块在读取时已被剥掉，
 * 一次只改标题就能把块整块抹掉（地图会从管理器的地图列表里消失）。
 *
 * 为什么 ZIP 在最前面：tag 与版本号本来就应该由文件名决定（`tag_v版本号.zip`），
 * 先选包才能把发布信息填对；附件必须挂在已存在的 Release 上，所以新建地图集时
 * 先建 Release（不带块），等第一个附件传成再写块 —— 先写块会让一张零版本的图
 * 立刻出现在管理器里。
 *
 * 「版本顺序」是这张地图的权威顺序：显示从新到旧、存储仍是旧→新，写进
 * `asset_versions` 数组；管理器据此判断谁是最新版 —— 数字比较排不出的
 * `3.4.1-fix` 与「资产版本号和 info 版本号不一致导致一直提示更新」都由它解决。
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
/** 新建模式下上传/建 Release 之后拿到的对象；上传与写块都挂在它上面。 */
const createdRelease = ref<PublishRelease | null>(null)

/**
 * 当前正在编辑的那张地图集的 Release。
 *
 * 「地图管理」按 tag 去最新列表里找（刷新后拿到的是新对象）；「新建地图集」
 * 在第一次写出 Release 之前是 null，之后按创建结果找 —— 上传与写块都需要
 * 一个真实存在的 Release id。
 */
const workingRelease = computed(() => {
  const tag = mode.value === 'manage' ? activeTag.value : (createdRelease.value?.tag ?? '')
  if (!tag) return null
  return releases.value.find((item) => item.tag === tag) ?? createdRelease.value
})

/** 已存在地图集的 tag：新建模式下第一次写出 Release 之后也固定下来。 */
const existingTag = computed(() => workingRelease.value?.tag ?? '')

/** 一张地图集「当前算哪一版」：排序表尾优先，其次附件名里的最大版本。 */
function latestVersionOfRelease(release: PublishRelease): string {
  const table = release.metadata?.asset_versions ?? []
  const last = table[table.length - 1]
  if (last?.mapset_version.trim()) return last.mapset_version.trim()
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
  /** 地图 ID（git tag）：一张地图一个，由 ZIP 文件名决定，创建后不能改。 */
  tag: string
  title: string
  body: string
  draft: boolean
  prerelease: boolean
}

/** 关系表的一行：左边 GUID、右边版本区间。 */
interface RelationRow {
  guid: string
  range: string
}

type RelationKind = 'requires' | 'compatible' | 'conflicts'

const RELATION_KINDS: readonly RelationKind[] = ['requires', 'compatible', 'conflicts']

/** 三张关系表的标题在 computed 里取，切语言时跟着变（不要在模板里 t(变量)）。 */
const relationKinds = computed(() =>
  RELATION_KINDS.map((id) => ({
    id,
    label:
      id === 'requires'
        ? t('mapPublish.requires')
        : id === 'compatible'
          ? t('mapPublish.compatible')
          : t('mapPublish.conflicts'),
  })),
)

interface MetadataDraft {
  levelSetName: string
  levelSetNameZh: string
  author: string
  nameI18n: I18nFieldDraft
  noteI18n: I18nFieldDraft
  groupI18n: I18nFieldDraft
  requires: RelationRow[]
  compatible: RelationRow[]
  conflicts: RelationRow[]
}

function blankMetadataDraft(): MetadataDraft {
  return {
    levelSetName: '',
    levelSetNameZh: '',
    author: '',
    nameI18n: blankI18nField(),
    noteI18n: blankI18nField(),
    groupI18n: blankI18nField(),
    requires: [],
    compatible: [],
    conflicts: [],
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

function recordToRows(record: Record<string, string>): RelationRow[] {
  return Object.entries(record).map(([guid, range]) => ({ guid, range }))
}

/**
 * 关系表存盘前的归一化：版本区间统一写成 `>=4.0.0`（后端解析器接受空格，
 * 但存进 block 时没必要保留）；空 GUID 的行直接丢掉，界面上加错了不算数。
 */
function relationsToRecord(rows: readonly RelationRow[]): Record<string, string> {
  const result: Record<string, string> = {}
  for (const row of rows) {
    const guid = row.guid.trim()
    if (!guid) continue
    result[guid] = row.range.trim().replace(/^([<>=!~]+)\s+/, '$1')
  }
  return result
}

function addRelationRow(kind: RelationKind) {
  metadataDraft[kind].push({ guid: '', range: '' })
}

function removeRelationRow(kind: RelationKind, index: number) {
  metadataDraft[kind].splice(index, 1)
}

function applyMetadata(metadata: MapPublishMetadata | null) {
  const draft = blankMetadataDraft()
  if (metadata) {
    draft.levelSetName = metadata.level_set_name
    draft.levelSetNameZh = metadata.level_set_name_zh
    draft.author = metadata.author
    draft.requires = recordToRows(metadata.requires)
    draft.compatible = recordToRows(metadata.compatible)
    draft.conflicts = recordToRows(metadata.conflicts)
  }
  const defaults: I18nDefaults = {
    levelSetName: draft.levelSetName,
    levelSetNameZh: draft.levelSetNameZh,
    author: draft.author,
  }
  draft.nameI18n = i18nDraftFrom('nameI18n', metadata?.name_i18n, defaults)
  draft.noteI18n = i18nDraftFrom('noteI18n', metadata?.note_i18n, defaults)
  draft.groupI18n = i18nDraftFrom('groupI18n', metadata?.group_i18n, defaults)
  Object.assign(metadataDraft, draft)
}

/** 文档有未保存改动：保存按钮旁边显示徽标。 */
const dirty = ref(false)
/** resetForm 正在灌数据：这一轮 watch 不该把表单标脏。 */
let applyingDocument = false

/**
 * 基本信息一改，没被作者动过的多语言行跟着变：显示名取地图集中英名、分组名取
 * 作者。动过的行不动 —— 作者手写的就是他要的。
 */
watch(
  () => [metadataDraft.levelSetName, metadataDraft.levelSetNameZh, metadataDraft.author],
  () => {
    if (applyingDocument) return
    const defaults = i18nDefaults()
    for (const field of I18N_FIELD_ORDER) {
      metadataDraft[field.id] = followDefaults(metadataDraft[field.id], field.id, defaults)
    }
  },
)

function resetForm(release: PublishRelease | null, metadata: MapPublishMetadata | null = release?.metadata ?? null) {
  applyingDocument = true
  form.tag = release?.tag ?? ''
  form.title = release?.title ?? ''
  form.body = release?.body ?? ''
  form.draft = release?.draft ?? false
  form.prerelease = release?.prerelease ?? false
  applyMetadata(metadata)
  // 表单写完后一拍再清脏标记：深 watch 在本轮同步代码跑完才执行
  void nextTick(() => {
    dirty.value = false
    applyingDocument = false
  })
}

/* ===== 版本排序表 ===== */

/**
 * 排序表 = 一批资产的版本号。**存储顺序永远是从旧到新**（`oc2-map` 块的
 * `asset_versions` 契约：末项是最新版）；界面显示时反转成「最新在最上」，
 * 反转只发生在组件渲染与拖动换算里（见 `VersionOrderTable`）。
 *
 * 表里**不存附件名**：附件名由资产规则渲染 `mapsetVersion` 得到（见 `assetNameOf`），
 * 所以改规则不需要重建整张表。`mapsetVersion` 可以写成 `3.4.1-fix`：数字比较排不出
 * 这种后缀，顺序只能由作者拖动决定。
 */
const assetRows = ref<AssetVersionRow[]>([])

/** 这一行对应的附件名：用当前资产规则渲染它的 mapsetVersion。 */
function assetNameOf(row: AssetVersionRow): string {
  return effectiveRule.value ? renderAssetName(effectiveRule.value, '', row.mapsetVersion) : ''
}

/**
 * 表里排过的按表顺序；没进表的（刚上传还没保存）按版本升序接在后面。
 *
 * 匹配附件的方式与后端 `synthesize_releases` 对齐：先按规则渲染出的文件名找，
 * 再退回「文件名里抽出的版本号等于表里的版本」。**找不到对应附件的表项直接丢**
 * —— 作者删掉一版后，表里留着一条指向空气的记录只会让下一次保存把它写回去。
 */
function versionRowsOf(release: PublishRelease | null): AssetVersionRow[] {
  const rule = effectiveRule.value
  const assets = release?.assets ?? []
  const byName = new Map(assets.map((asset) => [asset.name, asset]))
  const byVersion = new Map<string, PublishAsset>()
  for (const asset of assets) {
    const version = (rule ? versionFromRule(rule, asset.name) : null) ?? versionFromAssetName(asset.name)
    if (version && !byVersion.has(version)) byVersion.set(version, asset)
  }
  const rows: AssetVersionRow[] = []
  for (const entry of release?.metadata?.asset_versions ?? []) {
    const mapsetVersion = entry.mapset_version.trim()
    if (!mapsetVersion) continue
    const rendered = rule ? renderAssetName(rule, '', mapsetVersion) : ''
    const asset = (rendered ? byName.get(rendered) : undefined) ?? byVersion.get(mapsetVersion)
    if (!asset) continue
    byName.delete(asset.name)
    byVersion.delete(mapsetVersion)
    rows.push({ mapsetVersion, infoVersion: entry.info_version.trim(), size: asset.size })
  }
  const rest = [...byName.values()].sort((left, right) =>
    compareMapVersions(versionFromAssetName(left.name), versionFromAssetName(right.name)),
  )
  for (const asset of rest) {
    rows.push({ mapsetVersion: versionFromAssetName(asset.name), infoVersion: '', size: asset.size })
  }
  return rows
}

/** 存储顺序的末项就是最新版：`latest_*` 两个字段都取自它。 */
const latestRow = computed(() => assetRows.value[assetRows.value.length - 1] ?? null)

/** 删一版：确认框在页面里，组件只负责把行交回来。 */
function removeVersionRow(row: AssetVersionRow) {
  void confirmDeleteAsset(assetNameOf(row))
}

// 文档脏标记：表单、资料草稿、排序表任何一处改动都算未保存。必须放在
// `assetRows` 之后 —— watch 的 getter 创建时立刻执行，提前引用会撞 TDZ。
watch(
  () => [form, metadataDraft, assetRows.value],
  () => {
    if (!applyingDocument) dirty.value = true
  },
  { deep: true },
)

function openManagement(release: PublishRelease) {
  activeTag.value = release.tag
  createdRelease.value = null
  rejectPending()
  lastUploaded.value = ''
  resetPublishSteps()
  resetForm(release)
  mode.value = 'manage'
}

/**
 * 新建地图集：从空白开始，靠拖进来的第一个 ZIP 回填 tag、名字、作者与依赖。
 *
 * 不从上一张图复制元数据：关卡集名与作者必须是被发布的那张图的，复制过来
 * 一旦忘了改，写出去的块会顶着别人的身份（消费端按身份判断是不是同一张图）。
 */
function openCreate() {
  if (!selectedRepository.value) return
  createdRelease.value = null
  rejectPending()
  lastUploaded.value = ''
  resetPublishSteps()
  resetForm(null, null)
  mode.value = 'create'
}

function backToList() {
  mode.value = 'list'
  activeTag.value = ''
  createdRelease.value = null
  rejectPending()
  lastUploaded.value = ''
  resetPublishSteps()
  resetForm(null, null)
}

/* 换仓库时旧表单没有意义：tag 与地图信息都属于上一个仓库。 */
watch(selectedFullName, backToList)

/* ===== 发布：一个动作 = 建/改 Release → 传附件 → 写块 ===== */

/** 当前正在跑的写动作；发布与保存资料共用，同一时刻只允许一个。 */
const stepBusy = ref<PublishStepId | null>(null)
/** 上一次上传成功后的附件名：留在发布卡上给作者确认。 */
const lastUploaded = ref('')

type PublishStepState = 'pending' | 'active' | 'done' | 'failed'

/**
 * 发布三段各自的进度。
 *
 * 作者只点一次「发布这一版」，这里负责让他看见卡在哪一段：失败的那一行变红，
 * 附件已经传成、只有写块失败时可以单独重试。
 */
const publishSteps = reactive<Record<PublishStepId, PublishStepState>>({
  release: 'pending',
  asset: 'pending',
  metadata: 'pending',
})

const pending = reactive({
  /** 当前待上传的文件路径（改过名就是缓存里的副本）。 */
  path: '',
  /** 当前文件名，只用于展示与「是否符合规则」的比较。 */
  fileName: '',
  /** 这一版的版本号文本：写进附件名，也写进排序表的 mapset_version。 */
  version: '',
  inspection: null as PublishZipInspection | null,
})

/** 正在把选中的 ZIP 复制成规范文件名。 */
const renaming = ref(false)

/** 这张地图已有的资产规则：块里写了就用块的，否则从现有附件名反推。 */
const existingRule = computed(() => {
  const declared = workingRelease.value?.metadata?.asset.trim() ?? ''
  if (declared) return declared
  const names = workingRelease.value?.assets.map((asset) => asset.name) ?? []
  return names.length ? assetRuleFrom('', names) : ''
})

/**
 * 这张地图的资产规则。
 *
 * 新地图由 ZIP 名里的 tag 组合（`tag_v${version}.zip`），之后不再改；已有地图
 * 沿用块里写的规则 —— 改规则会让历史附件全部抽不出版本号。
 */
const effectiveRule = computed(() => {
  if (existingRule.value) return existingRule.value
  const tag = form.tag.trim() || defaultMapTag(pending.inspection?.level_set_name ?? '')
  return tag ? composeAssetRule(tag) : ''
})

/** 按规则渲染出的目标文件名：这就是附件名，也是本地副本的名字。 */
const canonicalName = computed(() => (effectiveRule.value ? renderAssetName(effectiveRule.value, '', pending.version) : ''))

/** 选中的文件名是否已经等于目标文件名；不等就先改名再上传。 */
const nameOk = computed(() => pending.fileName !== '' && canonicalName.value !== '' && pending.fileName === canonicalName.value)

/*
 * 附件集合或资产规则一变就重建排序表：上传、删除、保存后刷新列表都走这里。
 * 键里带上表里的 mapset 版本顺序，保存过的新顺序才会生效；单纯拖动不触发。
 * 必须放在 `effectiveRule` 之后：`immediate` 会在 setup 期间立刻读它。
 */
watch(
  () => {
    const release = workingRelease.value
    if (!release) return ''
    return [
      release.id,
      effectiveRule.value,
      release.assets.map((asset) => asset.name).join('|'),
      (release.metadata?.asset_versions ?? []).map((entry) => entry.mapset_version).join('|'),
    ].join('::')
  },
  () => {
    assetRows.value = versionRowsOf(workingRelease.value)
  },
  { immediate: true },
)

/** 文件名里能读出的版本号：优先按规则抽，其次按 `_v` 约定，最后退回数字段。 */
function versionFromFileName(fileName: string): string {
  return (
    versionFromRule(effectiveRule.value, fileName) ??
    parseVersionedZipName(fileName)?.version ??
    versionFromAssetName(fileName)
  )
}

const busyAny = computed(
  () =>
    stepBusy.value !== null ||
    savingRelease.value ||
    uploading.value ||
    inspecting.value ||
    renaming.value,
)

/** 三段里有没有跑过：跑过才显示进度，免得新建页一进来就三行「待处理」。 */
const publishVisible = computed(() => PUBLISH_STEP_ORDER.some((step) => publishSteps[step] !== 'pending'))
/** 有失败的一段：发布卡里出现「重试」按钮。 */
const hasFailedStep = computed(() => PUBLISH_STEP_ORDER.some((step) => publishSteps[step] === 'failed'))
/** 换地图 / 回列表时把上一张图的进度清掉。 */
function resetPublishSteps() {
  for (const step of PUBLISH_STEP_ORDER) publishSteps[step] = 'pending'
}
/** 发布按钮的可用性：还缺输入或正在忙就点不动。 */
const canPublish = computed(() => !busyAny.value && missingInputs.value.length === 0)

/**
 * 发布还缺的输入；空数组才允许点「发布这一版」。
 *
 * 这里**不看** Release 是否已存在：新建地图集时正是要去建它，把它算进前置
 * 会让新建页永远点不动。
 */
const missingInputs = computed(() =>
  missingPublishInputs({
    tag: form.tag,
    // 附件名由规则渲染，本地文件叫什么都能传：能算出版本号就算齐了
    assetName: canonicalName.value,
    versionText: pending.version,
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
  pending.fileName = ''
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
 * 名字 / 作者 / 依赖只在还空着时填：作者可能已经手写过，不能被第二个 ZIP
 * 覆盖。tag 与版本号相反，它们描述的就是「这个包」，直接覆盖。
 */
function applyInspection(result: PublishZipInspection) {
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
  if (metadataDraft.requires.length === 0 && Object.keys(result.requires).length > 0) {
    metadataDraft.requires = recordToRows(result.requires)
  }
}

/**
 * 拖入 / 选择本地 ZIP：只识别并算出版本号，不上传（上传是这一卡的按钮）。
 *
 * 已有地图集的 tag 不跟着文件名走：文件名里的 xxx 是作者随手起的，tag 才是
 * 这张图的身份，必须保持原值，靠改名把文件名对齐到规则。
 */
async function loadPendingZip(path: string) {
  pending.path = path
  pending.fileName = path.split(/[\\/]/).pop() ?? path
  pending.version = ''
  pending.inspection = null
  const result = await inspectZip(path)
  if (!result) {
    rejectPending()
    return
  }
  pending.inspection = result
  if (result.file_name.trim()) pending.fileName = result.file_name.trim()
  if (!existingTag.value) {
    const parsed = parseVersionedZipName(pending.fileName)
    if (parsed) form.tag = parsed.tag
  }
  pending.version = versionFromFileName(pending.fileName) || result.info_version.trim()
  applyInspection(result)
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
 * 把选中的 ZIP 复制成规范文件名（`tag_v版本号.zip`）。
 *
 * 不重命名用户的原始文件：作者手里的包可能叫别的名字，改名只是为了附件名
 * 符合 `asset` 规则；后端把副本放到缓存目录，上传读这份。
 */
async function renamePendingZip() {
  const name = canonicalName.value
  if (!pending.path || !name) return
  if (!isVersionText(pending.version.trim())) {
    message.warning(t('mapPublish.invalidVersion', { value: pending.version }))
    return
  }
  renaming.value = true
  try {
    const staged = await api.stagePublishZip(pending.path, name)
    pending.path = staged
    pending.fileName = name
    message.success(t('mapPublish.zipRenamed', { name }))
  } catch (reason) {
    message.error(backendMessageOf(reason))
  } finally {
    renaming.value = false
  }
}

/**
 * 新建 Release：只写标题 / 正文 / 状态，**不带 oc2-map 块**。
 *
 * 块要等第一个附件传成功之后再写（见 `publishVersion`）：先写块会让一张零版本
 * 的图立刻出现在管理器的地图列表里。新建时先查一遍同名 tag —— GitHub 允许在
 * 已有 tag 上建 Release，那样会把另一张图的附件挂到同一个身份下。
 */
async function createReleaseOnly(): Promise<boolean> {
  if (!selectedRepository.value) {
    message.warning(t('mapPublish.noSelection'))
    return false
  }
  const tag = form.tag.trim()
  if (!tag) {
    message.warning(t('mapPublish.tagHint'))
    return false
  }
  if (releases.value.some((item) => item.tag === tag)) {
    message.warning(t('mapPublish.tagExists', { tag }))
    return false
  }
  const saved = await saveRelease({
    releaseId: null,
    tag,
    title: form.title.trim(),
    body: form.body,
    draft: form.draft,
    prerelease: form.prerelease,
  })
  if (!saved) return false
  createdRelease.value = saved
  activeTag.value = saved.tag
  form.tag = saved.tag
  return true
}

/**
 * 把选中的 ZIP 传成 Release 附件（版本号在文件名里）。
 *
 * 附件必须挂在已存在的 Release 上；调用方保证 `workingRelease` 已经就绪。
 */
async function uploadPending(): Promise<boolean> {
  const zip = pending.inspection
  if (!pending.path || !zip) {
    message.warning(t('mapPublish.needZip'))
    return false
  }
  if (!canonicalName.value) {
    message.warning(t('mapPublish.zipNameInvalid'))
    return false
  }
  const version = pending.version.trim()
  if (!isVersionText(version)) {
    message.warning(t('mapPublish.invalidVersion', { value: version }))
    return false
  }
  const release = workingRelease.value
  if (!release) return false
  const name = canonicalName.value
  const infoVersion = zip.info_version.trim()
  const ok = await uploadAsset(release, pending.path, name)
  if (!ok) return false
  lastUploaded.value = name
  rejectPending()
  // 上传会重刷 Release 列表，排序表随之重建：等它落地再补这一版的 info 版本号。
  await nextTick()
  const row = assetRows.value.find((item) => item.mapsetVersion === version)
  if (row) {
    if (infoVersion) row.infoVersion = infoVersion
  } else {
    assetRows.value.push({ mapsetVersion: version, infoVersion, size: 0 })
  }
  return true
}

/**
 * 保存地图资料：标题 / 正文 / 状态 + `oc2-map` 块，**唯一的写块入口**。
 *
 * 绝不发「不带 metadata 的 PATCH」：后端在 metadata 为空时原样写正文，而正文里
 * 的块在读取时已经被剥掉 —— 那样一次「只改标题」就会把块整块抹掉，地图直接从
 * 管理器的地图列表里消失。
 */
async function saveDocument(): Promise<boolean> {
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
  if (i18nMissing.value.length > 0) {
    message.warning(t('mapPublish.i18nRequiredMissing', { fields: i18nMissing.value.join(' / ') }))
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
    if (!saved) return false
    resetForm(saved)
    return true
  } finally {
    stepBusy.value = null
  }
}

/** 进度行的状态文案：active 单独一档，其余复用已有的「已完成 / 待处理」。 */
function publishStateText(state: PublishStepState): string {
  switch (state) {
    case 'active':
      return t('mapPublish.stepActive')
    case 'done':
      return t('mapPublish.stepDone')
    case 'failed':
      return t('mapPublish.stepFailed')
    case 'pending':
      return t('mapPublish.stepPending')
  }
}

/** 当前基本信息算出的默认值：作者没动过的行跟着它走。 */
function i18nDefaults(): I18nDefaults {
  return {
    levelSetName: metadataDraft.levelSetName,
    levelSetNameZh: metadataDraft.levelSetNameZh,
    author: metadataDraft.author,
  }
}

/** 改一行：写值并记住这行是作者自己写的，默认值不再覆盖它。 */
function setI18nValue(field: I18nFieldId, language: string, value: string) {
  const draft = metadataDraft[field]
  draft.values[language] = value
  draft.touched[language] = true
}

/**
 * 填了多语言就必须写 zh-CN 与 en —— 回退链只在这两种齐全时才是确定的；
 * 三张表全空不算「填了」，保存时按默认值补。
 */
const i18nMissing = computed(() => {
  const missing: string[] = []
  for (const field of I18N_FIELD_ORDER) {
    const lacks = missingRequiredLanguages(metadataDraft[field.id])
    if (lacks.length) missing.push(`${t(field.labelKey)}（${lacks.join(' / ')}）`)
  }
  return missing
})

/** 草稿 → 块里的表；整张表为空时退回默认值（只写 zh-CN 与 en）。 */
function buildI18n(): Pick<MapPublishMetadata, 'name_i18n' | 'group_i18n' | 'note_i18n'> {
  const defaults = i18nDefaults()
  const pick = (field: I18nFieldId) => {
    const table = i18nTableOf(metadataDraft[field])
    return Object.keys(table).length ? table : defaultI18nValues(field, defaults)
  }
  return { name_i18n: pick('nameI18n'), group_i18n: pick('groupI18n'), note_i18n: pick('noteI18n') }
}

function buildMetadata(): MapPublishMetadata {
  const rows = assetRows.value.filter((row) => row.mapsetVersion.trim())
  const i18n = buildI18n()
  return {
    level_set_name: metadataDraft.levelSetName.trim(),
    level_set_name_zh: metadataDraft.levelSetNameZh.trim(),
    author: metadataDraft.author.trim(),
    name_i18n: i18n.name_i18n,
    group_i18n: i18n.group_i18n,
    note_i18n: i18n.note_i18n,
    asset: effectiveRule.value,
    // 两个派生字段以排序表为准：表尾就是最新版。后端保存时会用同一口径覆盖。
    latest_mapset_version: latestRow.value?.mapsetVersion.trim() ?? '',
    latest_info_version: latestRow.value?.infoVersion.trim() ?? '',
    asset_versions: rows.map((row) => ({
      mapset_version: row.mapsetVersion.trim(),
      info_version: row.infoVersion.trim(),
    })),
    requires: relationsToRecord(metadataDraft.requires),
    compatible: relationsToRecord(metadataDraft.compatible),
    conflicts: relationsToRecord(metadataDraft.conflicts),
  }
}

/**
 * 发布这一版：一次点按走完 建/改 Release → 传附件 → 写块。
 *
 * 任一段失败就停在那一段：附件没传成时块不写（否则块里会宣布一个下载不到的
 * 版本）；附件传成、只有写块失败时可以单独重试写块。
 */
async function publishVersion(): Promise<boolean> {
  if (busyAny.value) return false
  if (missingInputs.value.length > 0) {
    message.warning(
      t('mapPublish.sequenceNeed', { steps: missingInputs.value.map(stepTitle).join(' / ') }),
    )
    return false
  }
  for (const step of PUBLISH_STEP_ORDER) publishSteps[step] = 'pending'
  let done = 0
  try {
    publishSteps.release = 'active'
    stepBusy.value = 'release'
    if (workingRelease.value) {
      publishSteps.release = 'done'
    } else {
      publishSteps.release = (await createReleaseOnly()) ? 'done' : 'failed'
      if (publishSteps.release === 'failed') return false
    }
    done += 1

    publishSteps.asset = 'active'
    stepBusy.value = 'asset'
    publishSteps.asset = (await uploadPending()) ? 'done' : 'failed'
    if (publishSteps.asset === 'failed') return false
    done += 1

    publishSteps.metadata = 'active'
    // 交还占用：saveDocument 自己会设 stepBusy，带着 'asset' 进去会被它的重入保护挡掉
    stepBusy.value = null
    publishSteps.metadata = (await saveDocument()) ? 'done' : 'failed'
    if (publishSteps.metadata === 'failed') return false
    done += 1

    message.success(t('mapPublish.publishDone'))
    return true
  } finally {
    stepBusy.value = null
    const failed = PUBLISH_STEP_ORDER.find((step) => publishSteps[step] === 'failed')
    if (failed) {
      message.warning(t('mapPublish.sequenceStopped', { step: stepTitle(failed), done }))
    }
  }
}

/** 发布卡里的「重试」：只有写块失败时补写块，其余从头再发一次。 */
async function retryFailed() {
  if (publishSteps.metadata === 'failed') {
    publishSteps.metadata = 'active'
    const saved = await saveDocument()
    publishSteps.metadata = saved ? 'done' : 'failed'
    return
  }
  if (publishSteps.release === 'failed' || publishSteps.asset === 'failed') {
    await publishVersion()
  }
}

/*
 * 窗口拖放由页面声明：地图管理与新建地图集两态都收 ZIP，拖进来只识别、不
 * 自动上传。提示条文案也跟着这页走 —— 这里松手是「读取待发布的 ZIP」，不是
 * 「安装」，默认那句会让作者以为松手就把包装进游戏。
 */
const { dragActive } = useFileDrop({
  accept: ['zip'],
  hint: () => t('mapPublish.dropToInspect'),
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

/** 删掉一个版本：排序表里这一行也会跟着消失（附件是唯一事实）。 */
function confirmDeleteAsset(name: string) {
  const asset = workingRelease.value?.assets.find((item) => item.name === name)
  if (!asset) return
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
        <!-- 一张地图集 = 身份条 + 两个动作：发布这一版 / 保存地图资料 -->
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
              <n-tag v-if="form.draft" size="tiny" type="warning" :bordered="false">
                {{ t('mapPublish.draft') }}
              </n-tag>
              <n-tag v-if="form.prerelease" size="tiny" type="info" :bordered="false">
                {{ t('mapPublish.prerelease') }}
              </n-tag>
            </div>
            <div class="head-actions">
              <span v-if="workingRelease" class="muted identity-versions">
                {{
                  t('mapPublish.identityVersions', {
                    count: workingRelease.assets.length,
                    latest: latestRow?.mapsetVersion || '—',
                  })
                }}
              </span>
              <n-button size="small" :disabled="busyAny" @click="backToList">
                {{ t('mapPublish.backToMapSets') }}
              </n-button>
            </div>
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
            <n-button v-if="hasFailedStep && !permissionFailure.rateLimited" size="small"
              :disabled="busyAny" @click="retryFailed">
              {{ t('mapPublish.retryStep') }}
            </n-button>
          </div>
        </div>

        <!-- ① 发布这一版：一次点按走完 建/改 Release → 传附件 → 写块 -->
        <div class="panel publish-card">
          <div class="panel-head">
            <h3 class="sub-title">{{ t('mapPublish.publishCardTitle') }}</h3>
            <span v-if="lastUploaded" class="muted uploaded-note">
              {{ t('mapPublish.uploadedAs', { name: lastUploaded }) }}
            </span>
          </div>
          <p class="hint no-top">{{ t('mapPublish.publishCardHint') }}</p>

          <div class="drop-zone" :class="{ active: dragActive }" role="button" tabindex="0"
            @click="pickPendingZip" @keydown.enter="pickPendingZip">
            <p class="drop-title">{{ t('mapPublish.dropZone') }}</p>
            <p class="hint no-top">{{ t('mapPublish.addAssetHint') }}</p>
            <p v-if="pending.fileName" class="mono picked-zip">{{ pending.fileName }}</p>
          </div>

          <div v-if="inspecting" class="state-line">
            <n-spin size="small" />
            <span>{{ t('mapPublish.inspectingZip') }}</span>
          </div>

          <template v-if="pending.inspection">
            <p class="field-section-title">{{ t('mapPublish.zipSectionRule') }}</p>
            <div class="form-grid">
              <div class="field">
                <span class="field-label">{{ t('mapPublish.zipTagLabel') }}</span>
                <n-input v-model:value="form.tag" size="small"
                  :placeholder="t('mapPublish.zipTagPlaceholder')"
                  :disabled="Boolean(existingTag) || busyAny" />
                <p v-if="existingTag" class="hint">
                  {{ t('mapPublish.zipTagLocked', { tag: existingTag }) }}
                </p>
              </div>
              <div class="field">
                <span class="field-label">{{ t('mapPublish.zipVersionLabel') }}</span>
                <n-input v-model:value="pending.version" size="small"
                  :placeholder="t('mapPublish.zipVersionPlaceholder')" :disabled="busyAny" />
              </div>
              <div class="field">
                <span class="field-label">{{ t('mapPublish.zipNameLabel') }}</span>
                <code class="mono rule-text">{{ canonicalName || '—' }}</code>
                <p v-if="nameOk" class="ok no-top">{{ t('mapPublish.zipNameOk') }}</p>
                <p v-else-if="canonicalName" class="hint">
                  {{ t('mapPublish.zipNameWillUploadAs', { name: canonicalName }) }}
                </p>
                <p v-else class="warn no-top">{{ t('mapPublish.zipNameInvalid') }}</p>
              </div>
              <div class="field">
                <span class="field-label">{{ t('mapPublish.zipRuleLabel') }}</span>
                <code class="mono rule-text">{{ effectiveRule || '—' }}</code>
                <p class="hint">
                  {{ existingRule ? t('mapPublish.zipRuleFromExisting') : t('mapPublish.zipRuleLocked') }}
                </p>
              </div>
            </div>

            <p class="field-section-title">{{ t('mapPublish.zipSectionInspection') }}</p>
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
          </template>

          <div class="actions">
            <n-button size="small" :loading="renaming"
              :disabled="busyAny || !pending.path || !canonicalName || nameOk" @click="renamePendingZip">
              {{ t('mapPublish.zipRename') }}
            </n-button>
            <n-button size="small" type="primary" :loading="stepBusy !== null"
              :disabled="!canPublish" @click="publishVersion">
              {{ workingRelease ? t('mapPublish.publishVersion') : t('mapPublish.publishFirstVersion') }}
            </n-button>
            <span class="muted">{{ sequenceHint }}</span>
          </div>

          <!-- 三段进度：跑过之后才出现，失败的那一行留在原地等重试 -->
          <ul v-if="publishVisible" class="publish-progress">
            <li v-for="step in PUBLISH_STEP_ORDER" :key="step" :class="publishSteps[step]">
              <span class="progress-dot" />
              <span class="progress-label">{{ stepTitle(step) }}</span>
              <span class="progress-state">{{ publishStateText(publishSteps[step]) }}</span>
            </li>
          </ul>
          <div v-if="hasFailedStep" class="actions">
            <n-button size="small" :disabled="busyAny" @click="retryFailed">
              {{ t('mapPublish.retryStep') }}
            </n-button>
          </div>
        </div>

        <!-- ② 地图资料：唯一写块入口（正文与 oc2-map 块一起写） -->
        <div class="panel document-card">
          <div class="panel-head">
            <h3 class="sub-title">{{ t('mapPublish.documentCardTitle') }}</h3>
            <n-tag v-if="dirty" size="tiny" type="warning" :bordered="false">
              {{ t('mapPublish.unsaved') }}
            </n-tag>
          </div>
          <p class="hint no-top">{{ t('mapPublish.documentCardHint') }}</p>
          <p v-if="!workingRelease" class="warn no-top">{{ t('mapPublish.documentNeedsRelease') }}</p>

          <p class="field-section-title">{{ t('mapPublish.metadataSectionBasic') }}</p>
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
          </div>

          <!-- 多语言：默认收起；一行一种语言，没填的行保存时按规则补，填了就必须有 zh-CN 与 en -->
          <n-collapse class="advanced">
            <n-collapse-item :title="t('mapPublish.i18nSettings')" name="i18n">
              <p class="hint no-top">{{ t('mapPublish.i18nAutoHint') }}</p>
              <p class="hint no-top">{{ t('mapPublish.i18nHint') }}</p>
              <div v-for="field in I18N_FIELD_ORDER" :key="field.id" class="i18n-field">
                <span class="field-label">{{ t(field.labelKey) }}</span>
                <div class="i18n-rows">
                  <label v-for="language in MAP_I18N_LANGUAGES" :key="language" class="i18n-row">
                    <span class="i18n-lang">{{ MAP_I18N_LANGUAGE_LABELS[language] }}</span>
                    <n-input size="small" :value="metadataDraft[field.id].values[language] ?? ''"
                      :disabled="busyAny"
                      @update:value="(value) => setI18nValue(field.id, language, value)" />
                  </label>
                </div>
                <p v-if="extraLanguages(metadataDraft[field.id]).length" class="hint no-top">
                  {{
                    t('mapPublish.i18nExtraKept', {
                      languages: extraLanguages(metadataDraft[field.id]).join(' / '),
                    })
                  }}
                </p>
              </div>
              <p v-if="i18nMissing.length" class="warn no-top">
                {{ t('mapPublish.i18nRequiredMissing', { fields: i18nMissing.join(' / ') }) }}
              </p>
            </n-collapse-item>
          </n-collapse>

          <!-- 版本顺序：显示从新到旧，存储仍是旧→新；拖动与两个版本号都写进块 -->
          <n-collapse class="advanced" :default-expanded-names="['versions']">
            <n-collapse-item :title="t('mapPublish.versionOrderSection')" name="versions">
              <p class="hint no-top">{{ t('mapPublish.versionOrderHint') }}</p>
              <p v-if="!assetRows.length" class="hint">{{ t('mapPublish.versionMapEmpty') }}</p>
              <VersionOrderTable v-else v-model:rows="assetRows" :name-of="assetNameOf" :disabled="busyAny"
                @remove="removeVersionRow" />
            </n-collapse-item>
          </n-collapse>

          <!-- 高级设置：关系表与派生版本号，默认收起 —— 大多数版本不用碰。
               assets 规则只在发布卡里展示，这里不再重复一份。 -->
          <n-collapse class="advanced">
            <n-collapse-item :title="t('mapPublish.advancedSettings')" name="advanced">
              <p class="hint no-top">{{ t('mapPublish.advancedHint') }}</p>

              <div class="form-grid">
                <div class="field">
                  <span class="field-label">{{ t('mapPublish.latestVersion') }}</span>
                  <code class="mono rule-text">
                    {{ latestRow?.mapsetVersion || '—' }} · {{ latestRow?.infoVersion || '—' }}
                  </code>
                  <p class="hint">{{ t('mapPublish.versionHint') }}</p>
                </div>
              </div>

              <div v-for="kind in relationKinds" :key="kind.id" class="relation-block">
                <div class="relation-head">
                  <span class="field-label">{{ kind.label }}</span>
                  <n-button size="tiny" secondary :disabled="busyAny" @click="addRelationRow(kind.id)">
                    {{ t('mapPublish.relationAddRow') }}
                  </n-button>
                </div>
                <p v-if="!metadataDraft[kind.id].length" class="hint no-top">
                  {{ t('mapPublish.relationEmpty') }}
                </p>
                <div v-for="(row, index) in metadataDraft[kind.id]" :key="index" class="relation-row">
                  <n-input v-model:value="row.guid" size="small"
                    :placeholder="t('mapPublish.relationGuidPlaceholder')" :disabled="busyAny" />
                  <n-input v-model:value="row.range" size="small"
                    :placeholder="t('mapPublish.relationRangePlaceholder')" :disabled="busyAny" />
                  <n-button size="small" quaternary circle :title="t('mapPublish.relationRemoveRow')"
                    :disabled="busyAny" @click="removeRelationRow(kind.id, index)">
                    <template #icon><TrashOutline width="15" height="15" /></template>
                  </n-button>
                </div>
              </div>
            </n-collapse-item>
          </n-collapse>

          <!-- GitHub 页面：标题 / 正文 / 状态。保存时与块一起写，只改正文不会把块冲掉 -->
          <n-collapse class="advanced">
            <n-collapse-item :title="t('mapPublish.githubPageSection')" name="github">
              <div class="form-grid">
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
            </n-collapse-item>
          </n-collapse>

          <div class="actions">
            <n-button size="small" type="primary" :loading="stepBusy === 'metadata'"
              :disabled="busyAny || !workingRelease || !metadataDraft.levelSetName.trim()"
              @click="saveDocument">
              {{ t('mapPublish.saveMetadata') }}
            </n-button>
            <span v-if="dirty" class="muted">{{ t('mapPublish.unsavedHint') }}</span>
          </div>
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
.version-list {
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

/* ===== 发布页：身份条 + 两个动作卡 ===== */

.publish-head { margin-bottom: 16px; }

.head-actions {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 8px;
}
.identity-versions { font-size: 12px; }

.publish-card,
.document-card { margin-top: 16px; }
.i18n-field { margin-top: 14px; }
.i18n-rows { display: flex; flex-direction: column; gap: 6px; }
.i18n-row { display: flex; align-items: center; gap: 10px; }
.i18n-lang { flex: 0 0 88px; font-size: 12px; color: var(--oc2-text-3); }

.publish-card .panel-head .sub-title,
.document-card .panel-head .sub-title { margin: 0; }

/* 三段进度：跑过之后才出现，失败的那一行留在原地等重试 */
.publish-progress {
  list-style: none;
  margin: 12px 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.publish-progress li {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12px;
  color: var(--oc2-text-3);
}
.publish-progress .progress-dot {
  flex: 0 0 auto;
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--oc2-row-line);
}
.publish-progress li.active { color: var(--oc2-text-1); }
.publish-progress li.active .progress-dot { background: var(--oc2-accent); }
.publish-progress li.done { color: var(--oc2-text-2); }
.publish-progress li.done .progress-dot { background: color-mix(in srgb, #16b364 78%, var(--oc2-text-1)); }
.publish-progress li.failed { color: color-mix(in srgb, #e5484d 82%, var(--oc2-text-1)); }
.publish-progress li.failed .progress-dot { background: color-mix(in srgb, #e5484d 82%, var(--oc2-text-1)); }
.publish-progress .progress-state { margin-left: auto; }

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
/* 只读派生值（资产规则 / 目标文件名 / tag）：等宽一行，太长就换行不撑宽 */
.rule-text {
  display: block;
  font-size: 12px;
  line-height: 1.5;
  overflow-wrap: anywhere;
}

/* 「符合规则」用绿色：主题色是琥珀，成功态得自己混一档（明暗两套都读得清） */
.ok {
  margin: 6px 0 0;
  font-size: 12px;
  line-height: 1.6;
  color: color-mix(in srgb, #16b364 78%, var(--oc2-text-1));
}

/* 高级设置 / 多语言：默认收起，展开后与上面的基础字段拉开一点距离 */
.advanced { margin-top: 14px; }

/* 分区标题：把「Tag 与版本」「ZIP 里的信息」「基础信息」从字段堆里分出来 */
.field-section-title {
  margin: 18px 0 0;
  padding-top: 14px;
  border-top: 1px solid var(--oc2-divider);
  font-size: 12px;
  font-weight: 650;
  color: var(--oc2-text-2);
}
.field-section-title + .form-grid,
.field-section-title + .inspection-grid { margin-top: 10px; }

/* 关系表：一行两个输入框（左 GUID、右版本区间）+ 删除 */
.relation-block { margin-top: 14px; }
.relation-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
  margin-bottom: 8px;
}
.relation-row {
  display: grid;
  grid-template-columns: minmax(0, 1.6fr) minmax(0, 1fr) auto;
  align-items: center;
  gap: 8px;
  margin-top: 6px;
}

</style>
