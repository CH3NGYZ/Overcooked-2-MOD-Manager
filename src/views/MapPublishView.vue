<script setup lang="ts">
import { computed, h, onMounted, reactive, ref, watch } from 'vue'
import { NButton, NCheckbox, NInput, NPopover, NSpin, NSwitch, NTag, useDialog } from 'naive-ui'
import { RefreshOutline, SettingsOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { api } from '../services/tauri'
import AppModal from '../components/AppModal.vue'
import PageLoading from '../components/PageLoading.vue'
import { useFileDrop } from '../composables/useFileDrop'
import { assetNameFor, useMapPublish } from '../composables/useMapPublish'
import { latestVersionOf, versionFromAssetName } from '../mapPublishAsset'
import { markDialogBusy } from '../dialogBusy'
import { formatFileSize } from '../repositoryAssets'
import { useNotify } from '../useNotify'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import type { MapPublishMetadata, PublishAsset, PublishRelease, PublishZipInspection } from '../types'

/**
 * 地图发布页 —— 作者把地图 ZIP 作为 GitHub Release 发出去的唯一入口。
 *
 * 口径：**一个 tag 就是一张地图集**。列表按地图集展示，进到某张地图集里
 * 才是「地图管理」：上半是它的 oc2-map 信息（会被写回 Release note），
 * 下半是这张地图历来的资产（一个资产 = 一个版本，版本号在文件名里）。
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
  publishing,
  inspecting,
  deletingAsset,
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
  publishVersion,
  deleteAsset,
} = useMapPublish()

onMounted(loadState)

/* ===== 视图：列表 / 地图管理（含新建） ===== */

type ViewMode = 'list' | 'manage' | 'create'

const mode = ref<ViewMode>('list')
/** 正在管理的地图集：用 tag 定位，`releases` 刷新后自动跟着换新对象。 */
const activeTag = ref('')

const activeRelease = computed(
  () => releases.value.find((item) => item.tag === activeTag.value) ?? null,
)

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
  /** 地图 ID（git tag）：一张地图一个，不能改。 */
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
  resetForm(release)
  mode.value = 'manage'
}

/**
 * 新建：用同仓库最近一版带 oc2-map 块的元数据回填。
 *
 * 作者通常只是「再建一张图」，关系表与附件模板都不变；从最近一版复制，
 * 比从空白开始填十来个字段省事。取第一个带 metadata 的而不是第一个
 * Release：最新一个可能不是地图。
 */
function openCreate() {
  if (!selectedRepository.value) return
  const template = releases.value.find((item) => item.metadata !== null) ?? null
  resetForm(null, template?.metadata ?? null)
  form.tag = ''
  mode.value = 'create'
}

function backToList() {
  mode.value = 'list'
  activeTag.value = ''
  addOpen.value = false
  resetPending()
  resetForm(null, null)
}

/* 换仓库时旧表单没有意义：tag 与地图信息都属于上一个仓库。 */
watch(selectedFullName, backToList)

/**
 * 保存地图信息。
 *
 * 新建时这里同时把 oc2-map 块写进 Release note；已有地图集只改信息。
 * 三个必填项先在前端拦一次拿到完整文案，真正不可绕过的校验仍在后端
 * （tag 规则、关系区间语法），失败时按 tag 翻译显示。
 */
async function submitSave() {
  if (!selectedRepository.value) {
    message.warning(t('mapPublish.noSelection'))
    return
  }
  const creating = mode.value === 'create'
  const release = activeRelease.value
  if (!creating && !release) return
  const tag = creating ? form.tag.trim() : release!.tag
  if (!tag) {
    message.warning(t('mapPublish.tagHint'))
    return
  }
  if (!metadataDraft.levelSetName.trim()) {
    message.warning(t('mapPublish.missingLevelSetName'))
    return
  }
  const saved = await saveRelease({
    releaseId: creating ? null : release!.id,
    tag,
    title: form.title.trim(),
    body: form.body,
    draft: form.draft,
    prerelease: form.prerelease,
    metadata: buildMetadata(),
  })
  if (!saved) return
  // 新建成功后直接进「地图管理」：下一步就是传这一版的资产。
  activeTag.value = saved.tag
  mode.value = 'manage'
  resetForm(saved)
}

/* ===== 新增资产：弹窗里拖入 ZIP → 识别 → 上传并补写 note ===== */

const addOpen = ref(false)
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

/** 会上传成什么名字：模板 + 这一版的版本号；模板留空时按关卡集名生成。 */
const pendingAssetName = computed(() => {
  const release = activeRelease.value
  if (!release || !pending.version.trim()) return ''
  return assetNameFor(release, pending.version)
})

function resetPending() {
  pending.path = ''
  pending.version = ''
  pending.inspection = null
}

function openAddAsset() {
  resetPending()
  addOpen.value = true
}

/** 识别结果只在字段还空着时回填，不覆盖作者已经写好的内容。 */
function applyInspection(result: PublishZipInspection) {
  if (!metadataDraft.levelSetName.trim() && result.level_set_name.trim()) {
    metadataDraft.levelSetName = result.level_set_name.trim()
  }
  if (!metadataDraft.levelSetNameZh.trim() && result.level_set_name_zh.trim()) {
    metadataDraft.levelSetNameZh = result.level_set_name_zh.trim()
  }
  if (!metadataDraft.author.trim() && result.author.trim()) {
    metadataDraft.author = result.author.trim()
  }
  if (!metadataDraft.latestInfoVersion.trim() && result.info_version.trim()) {
    metadataDraft.latestInfoVersion = result.info_version.trim()
  }
  if (Object.keys(parsePairs(metadataDraft.requires)).length === 0 && Object.keys(result.requires).length > 0) {
    metadataDraft.requires = pairsToText(result.requires)
  }
}

async function loadPendingZip(path: string) {
  pending.path = path
  pending.inspection = null
  const result = await inspectZip(path)
  if (!result) return
  pending.inspection = result
  // ZIP 名里的版本 = 这一版的版本号；info 里的版本另写进元数据。
  pending.version = result.mapset_version.trim()
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

async function submitAddAsset() {
  const release = activeRelease.value
  const inspectionResult = pending.inspection
  if (!release || !pending.path || !inspectionResult) return
  const ok = await publishVersion(release, pending.path, pending.version.trim(), inspectionResult)
  if (!ok) return
  addOpen.value = false
  resetPending()
  // 上传顺带补写了 note（附件模板、最新版本、依赖），把表单同步成新值。
  const fresh = releases.value.find((item) => item.tag === release.tag)
  if (fresh) resetForm(fresh)
}

function requiresText(record: Record<string, string>): string {
  return Object.entries(record)
    .map(([key, value]) => `${key} ${value}`)
    .join(', ')
}

/*
 * 窗口拖放由页面声明，弹窗只负责显示：拖进窗口就打开新增资产并直接识别。
 * 列表页没有「当前地图集」，拖进来不做事（全窗提示条仍会说明只收 ZIP）。
 */
const { dragActive } = useFileDrop({
  accept: ['zip'],
  busy: () => mode.value !== 'manage' || inspecting.value || publishing.value,
  onDrop: (path) => {
    if (mode.value !== 'manage') return
    addOpen.value = true
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
        <!-- 上半：这张地图集的信息（写回 Release note 的 oc2-map 块） -->
        <div class="panel">
          <div class="panel-head">
            <div class="manage-head">
              <code class="release-tag mono">{{ mode === 'create' ? (form.tag || t('mapPublish.newMapSet')) : (activeRelease?.tag ?? '') }}</code>
              <span class="release-title">
                {{ mode === 'create' ? '' : (activeRelease ? displayName(activeRelease) : '') }}
              </span>
            </div>
            <n-button size="small" :disabled="savingRelease" @click="backToList">
              {{ t('mapPublish.backToMapSets') }}
            </n-button>
          </div>
          <p class="hint no-top">{{ t('mapPublish.manageHint') }}</p>
          <p v-if="mode === 'manage' && !activeRelease?.metadata" class="warn no-top">
            {{ t('mapPublish.noMetadata') }}
          </p>

          <div class="form-grid">
            <div class="field">
              <span class="field-label">{{ t('mapPublish.tagLabel') }}</span>
              <n-input v-model:value="form.tag" size="small" placeholder="chan"
                :disabled="mode === 'manage' || savingRelease" />
              <p class="hint">{{ t('mapPublish.tagHint') }}</p>
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.titleLabel') }}</span>
              <n-input v-model:value="form.title" size="small" :disabled="savingRelease" />
            </div>
            <div class="field span-2">
              <span class="field-label">{{ t('mapPublish.bodyLabel') }}</span>
              <n-input v-model:value="form.body" type="textarea" :autosize="{ minRows: 4, maxRows: 12 }"
                :disabled="savingRelease" />
              <p class="hint">{{ t('mapPublish.bodyHint') }}</p>
            </div>
            <div class="field span-2 switch-field">
              <label class="switch-label">
                <n-switch v-model:value="form.draft" size="small" :disabled="savingRelease" />
                <span>{{ t('mapPublish.draft') }}</span>
              </label>
              <label class="switch-label">
                <n-switch v-model:value="form.prerelease" size="small" :disabled="savingRelease" />
                <span>{{ t('mapPublish.prerelease') }}</span>
              </label>
            </div>
          </div>

          <div class="section-divider" />

          <h3 class="sub-title">{{ t('mapPublish.metadata') }}</h3>
          <p class="hint no-top">{{ t('mapPublish.metadataHint') }}</p>

          <div class="form-grid">
            <div class="field">
              <span class="field-label">{{ t('mapPublish.levelSetName') }}</span>
              <n-input v-model:value="metadataDraft.levelSetName" size="small" :disabled="savingRelease" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.levelSetNameZh') }}</span>
              <n-input v-model:value="metadataDraft.levelSetNameZh" size="small" :disabled="savingRelease" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.author') }}</span>
              <n-input v-model:value="metadataDraft.author" size="small" :disabled="savingRelease" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.assetTemplate') }}</span>
              <n-input v-model:value="metadataDraft.asset" size="small" placeholder="gaoda-${version}.zip"
                :disabled="savingRelease" />
              <p class="hint">{{ t('mapPublish.assetHint') }}</p>
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.latestMapsetVersion') }}</span>
              <n-input v-model:value="metadataDraft.latestMapsetVersion" size="small" :disabled="savingRelease" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.latestInfoVersion') }}</span>
              <n-input v-model:value="metadataDraft.latestInfoVersion" size="small" :disabled="savingRelease" />
            </div>
            <div class="field span-2">
              <p class="hint no-top">{{ t('mapPublish.versionHint') }}</p>
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.nameI18n') }}</span>
              <n-input v-model:value="metadataDraft.nameI18n" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="savingRelease" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.groupI18n') }}</span>
              <n-input v-model:value="metadataDraft.groupI18n" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="savingRelease" />
            </div>
            <div class="field span-2">
              <span class="field-label">{{ t('mapPublish.noteI18n') }}</span>
              <n-input v-model:value="metadataDraft.noteI18n" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="savingRelease" />
              <p class="hint">{{ t('mapPublish.i18nHint') }}</p>
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.requires') }}</span>
              <n-input v-model:value="metadataDraft.requires" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="savingRelease" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.compatible') }}</span>
              <n-input v-model:value="metadataDraft.compatible" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="savingRelease" />
            </div>
            <div class="field">
              <span class="field-label">{{ t('mapPublish.conflicts') }}</span>
              <n-input v-model:value="metadataDraft.conflicts" type="textarea" :autosize="{ minRows: 3, maxRows: 8 }"
                :disabled="savingRelease" />
            </div>
            <div class="field">
              <p class="hint align-end">{{ t('mapPublish.relationHint') }}</p>
            </div>
          </div>

          <div class="actions">
            <n-button size="small" type="primary" :loading="savingRelease" :disabled="savingRelease || !metadataDraft.levelSetName.trim()"
              @click="submitSave">
              {{ savingRelease ? t('mapPublish.saving') : t('mapPublish.save') }}
            </n-button>
            <n-button size="small" :disabled="savingRelease" @click="backToList">
              {{ t('mapPublish.backToMapSets') }}
            </n-button>
          </div>
        </div>

        <!-- 下半：资产（一个资产 = 一个版本，版本号在文件名里） -->
        <div v-if="mode === 'manage'" class="panel assets-panel">
          <div class="panel-head">
            <h3 class="sub-title">{{ t('mapPublish.assets') }}</h3>
            <n-button size="small" type="primary" :disabled="!activeRelease?.metadata || inspecting || publishing"
              @click="openAddAsset">
              {{ t('mapPublish.addAsset') }}
            </n-button>
          </div>
          <p class="hint no-top">{{ t('mapPublish.assetsHint') }}</p>

          <p v-if="!activeRelease?.assets.length" class="hint">{{ t('mapPublish.assetEmpty') }}</p>
          <ul v-else class="asset-list">
            <li v-for="asset in activeRelease?.assets ?? []" :key="asset.id" class="asset-row">
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

  <!--
    新增资产：拖入本地 ZIP（也走窗口拖放）→ 识别文件名版本 / info 版本 /
    requires.txt → 填这一版的版本号 → 上传并补写 Release note。
  -->
  <AppModal :show="addOpen" :title="t('mapPublish.addAsset')" max-width="min(640px, 92%)"
    @update:show="(value: boolean) => (addOpen = value)">
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
      <div class="section-divider" />
      <h4 class="sub-title">{{ t('mapPublish.inspectionTitle') }}</h4>
      <div class="inspection-grid">
        <div>
          <span class="field-label">{{ t('mapPublish.inspectionFile') }}</span>
          <span class="mono">{{ pending.inspection.file_name }}</span>
        </div>
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
          <n-input v-model:value="pending.version" size="small" placeholder="1.2.0" :disabled="publishing" />
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

    <template #footer>
      <n-button size="small" :disabled="publishing" @click="addOpen = false">
        {{ t('common.action.cancel') }}
      </n-button>
      <n-button size="small" type="primary" :loading="publishing"
        :disabled="publishing || inspecting || !pendingAssetName" @click="submitAddAsset">
        {{ t('mapPublish.publishAsset') }}
      </n-button>
    </template>
  </AppModal>
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
</style>
