<script setup lang="ts">
import { computed, h, onMounted, reactive, ref, watch } from 'vue'
import { NButton, NCheckbox, NInput, NSelect, NSpin, NSwitch, NTag, useDialog } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { api } from '../services/tauri'
import PageLoading from '../components/PageLoading.vue'
import { assetNameFor, useMapPublish } from '../composables/useMapPublish'
import { markDialogBusy } from '../dialogBusy'
import { formatFileSize } from '../repositoryAssets'
import { useNotify } from '../useNotify'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import type { MapPublishMetadata, PublishAsset, PublishRelease } from '../types'

/**
 * 地图发布页 —— 作者把地图 ZIP 作为 GitHub Release 发出去的唯一入口。
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
} = useMapPublish()

onMounted(loadState)

/* ===== 元数据表单 ===== */

interface FormState {
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

const formOpen = ref(false)
/** null = 新建；有值 = 正在改这一版（附件区也只在这种状态下出现）。 */
const editingId = ref<number | null>(null)
const pickedZip = ref<string | null>(null)

const editingRelease = computed(() =>
  editingId.value === null ? null : releases.value.find((item) => item.id === editingId.value) ?? null,
)

const assetPreview = computed(() => {
  const template = metadataDraft.asset.trim()
  const tag = form.tag.trim()
  if (!template || !tag) return ''
  return template.split('${version}').join(tag)
})

const expectedAssetName = computed(() => (editingRelease.value ? assetNameFor(editingRelease.value) : ''))

const pickedZipName = computed(() => {
  const path = pickedZip.value
  if (!path) return ''
  return path.split(/[\\/]/).pop() ?? path
})

const repositoryOptions = computed(() =>
  repositories.value.map((repository) => ({
    label: repository.private
      ? `${repository.full_name} · ${t('mapPublish.repositoryPrivate')}`
      : repository.full_name,
    value: repository.full_name,
  })),
)

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
  editingId.value = release?.id ?? null
  form.tag = release?.tag ?? ''
  form.title = release?.title ?? ''
  form.body = release?.body ?? ''
  form.draft = release?.draft ?? false
  form.prerelease = release?.prerelease ?? false
  applyMetadata(metadata)
  pickedZip.value = null
}

/**
 * 新建时用同仓库最新一版的元数据回填。
 *
 * 作者通常只是「再发一版」，英文名、作者、关系表都不变；从最近一版带
 * oc2-map 块的 Release 复制，比从空白开始填十来个字段省事得多。
 * 取第一个带 metadata 的而不是第一个 Release：最新一个可能不是地图。
 */
function startNew() {
  if (!selectedRepository.value) return
  const template = releases.value.find((item) => item.metadata !== null) ?? null
  resetForm(null, template?.metadata ?? null)
  formOpen.value = true
}

function startEdit(release: PublishRelease) {
  resetForm(release)
  formOpen.value = true
}

function closeForm() {
  formOpen.value = false
  editingId.value = null
  pickedZip.value = null
}

/* 换仓库时旧表单没有意义：tag 与版本号都属于上一个仓库。 */
watch(selectedFullName, closeForm)

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

/**
 * 保存。
 *
 * 三个必填项先在前端拦一次，拿到的是完整文案；真正不可绕过的校验仍在后端
 * （tag 规则、关系区间语法），失败时按 tag 翻译显示。
 */
async function submitSave() {
  if (!selectedRepository.value) {
    message.warning(t('mapPublish.noSelection'))
    return
  }
  const tag = form.tag.trim()
  if (!tag) {
    message.warning(t('mapPublish.tagHint'))
    return
  }
  if (!metadataDraft.levelSetName.trim()) {
    message.warning(t('mapPublish.missingLevelSetName'))
    return
  }
  const saved = await saveRelease({
    releaseId: editingId.value,
    tag,
    title: form.title.trim(),
    body: form.body,
    draft: form.draft,
    prerelease: form.prerelease,
    metadata: buildMetadata(),
  })
  // 保存后停留在表单里：新版本的附件要接着传，作者不用再从列表点回来
  if (saved) resetForm(saved)
}

/* ===== 附件 ===== */

async function chooseZip() {
  try {
    const path = await api.chooseLevelSetZip()
    if (path) pickedZip.value = path
  } catch (reason) {
    message.error(backendMessageOf(reason))
  }
}

async function uploadZip() {
  const release = editingRelease.value
  const path = pickedZip.value
  if (!release || !path) return
  if (await uploadAsset(release, path)) pickedZip.value = null
}

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

/** 仓库可见范围由 GitHub App 安装时勾选的仓库决定，列表为空时这是唯一出路。 */
function openInstall() {
  const url = auth.value?.install_url
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
      if (ok && editingId.value === release.id) closeForm()
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
  </Teleport>

  <PageLoading :show="authLoading">
    <template #status>{{ t('common.loading') }}</template>

    <p class="page-intro muted">{{ t('mapPublish.subtitle') }}</p>

    <!-- 登录：GitHub App 用户授权（系统浏览器 + 本机回调），这里只显示进度与入口 -->
    <section class="group">
      <h2 class="group-title">{{ t('mapPublish.signIn') }}</h2>
      <div class="panel">
        <div v-if="!authReady" class="state-line">
          <n-spin size="small" />
          <span>{{ t('common.loading') }}</span>
        </div>

        <template v-else-if="authenticated">
          <div class="signed-in">
            <span class="signed-in-text">{{ t('mapPublish.signedInAs', { login: auth?.login ?? '' }) }}</span>
            <n-button size="small" :loading="signingOut" :disabled="signingOut" @click="signOut">
              {{ t('mapPublish.signOut') }}
            </n-button>
          </div>
          <div class="auth-block">
            <p class="hint no-top">{{ t('mapPublish.installAppHint') }}</p>
            <div class="code-actions">
              <n-button size="small" @click="openInstall">{{ t('mapPublish.installApp') }}</n-button>
            </div>
          </div>
        </template>

        <div v-else-if="authorization" class="auth-block">
          <p class="hint no-top">{{ t('mapPublish.waiting') }}</p>
          <code class="auth-url mono">{{ authorization.authorize_url }}</code>
          <div class="code-actions">
            <n-button size="small" @click="reopenAuthorization">{{ t('mapPublish.reopenBrowser') }}</n-button>
            <n-button size="small" @click="cancelSignIn">{{ t('mapPublish.cancelSignIn') }}</n-button>
          </div>
        </div>

        <div v-else class="auth-block">
          <p class="hint no-top">{{ t('mapPublish.signInHint') }}</p>
          <p v-if="auth && !auth.secret_configured" class="warn no-top">{{ t('mapPublish.secretMissing') }}</p>
          <p v-else-if="authPhase === 'expired'" class="warn no-top">{{ t('mapPublish.expired') }}</p>
          <p v-else-if="authPhase === 'denied'" class="warn no-top">{{ t('mapPublish.denied') }}</p>
          <p v-else-if="authPhase === 'failed'" class="warn no-top">{{ authFailure }}</p>
          <div>
            <n-button size="small" type="primary" :loading="signingIn"
              :disabled="signingIn || (auth !== null && !auth.secret_configured)" @click="startSignIn">
              {{ t('mapPublish.signIn') }}
            </n-button>
          </div>
        </div>
      </div>
    </section>

    <!-- 仓库：只列有 push 权限的，map-source 由后端排前 -->
    <section v-if="authReady && authenticated" class="group">
      <h2 class="group-title">{{ t('mapPublish.repositories') }}</h2>
      <div class="panel">
        <div class="repo-row">
          <n-select class="repo-select" size="small" :value="selectedFullName" :options="repositoryOptions"
            :placeholder="t('mapPublish.selectRepository')" :disabled="repositoriesLoading"
            @update:value="selectRepository" />
          <n-button size="small" :disabled="creatingRepository" @click="createOpen = !createOpen">
            {{ t('mapPublish.createRepository') }}
          </n-button>
        </div>
        <p class="hint">{{ t('mapPublish.repositoryHint') }}</p>
        <p v-if="!repositoriesLoading && !repositories.length" class="hint">{{ t('mapPublish.repositoryEmpty') }}</p>

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
            <n-button size="small" type="primary" :loading="creatingRepository" :disabled="creatingRepository"
              @click="submitCreateRepository">
              {{ t('mapPublish.create') }}
            </n-button>
            <n-button size="small" :disabled="creatingRepository" @click="createOpen = false">
              {{ t('common.action.cancel') }}
            </n-button>
          </div>
        </div>
      </div>
    </section>

    <!-- 版本列表 / 编辑表单：同一块区域，二者只出现一个 -->
    <section v-if="authReady && authenticated" class="group">
      <h2 class="group-title">
        {{ formOpen ? (editingId === null ? t('mapPublish.newRelease') : t('mapPublish.editRelease')) : t('mapPublish.releases') }}
      </h2>

      <div v-if="formOpen" class="panel">
        <div class="form-grid">
          <div class="field">
            <span class="field-label">{{ t('mapPublish.tagLabel') }}</span>
            <n-input v-model:value="form.tag" size="small" placeholder="1.2.0" :disabled="savingRelease" />
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
        <p v-if="editingId !== null && !editingRelease?.metadata" class="warn no-top">
          {{ t('mapPublish.noMetadata') }}
        </p>

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
            <p v-if="assetPreview" class="hint">{{ t('mapPublish.assetExpected', { name: assetPreview }) }}</p>
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

        <!-- 附件：必须先保存出 release_id，上传才有落点 -->
        <template v-if="editingId !== null">
          <div class="section-divider" />
          <h3 class="sub-title">{{ t('mapPublish.assets') }}</h3>
          <p class="hint no-top">{{ t('mapPublish.assetsHint') }}</p>

          <p v-if="!editingRelease?.assets.length" class="hint">{{ t('mapPublish.assetEmpty') }}</p>
          <ul v-else class="asset-list">
            <li v-for="asset in editingRelease?.assets ?? []" :key="asset.id" class="asset-row">
              <code class="mono">{{ asset.name }}</code>
              <span class="muted">{{ formatFileSize(asset.size) }}</span>
              <n-button size="tiny" type="error" secondary :disabled="deletingAsset"
                @click="confirmDeleteAsset(asset)">
                {{ t('mapPublish.deleteAsset') }}
              </n-button>
            </li>
          </ul>

          <div class="upload-row">
            <n-button size="small" :disabled="uploading" @click="chooseZip">{{ t('mapPublish.chooseZip') }}</n-button>
            <span v-if="pickedZipName" class="muted mono picked-zip" :title="pickedZip ?? ''">{{ pickedZipName }}</span>
            <n-button size="small" type="primary" :loading="uploading" :disabled="uploading || !pickedZip"
              @click="uploadZip">
              {{ uploading ? t('mapPublish.uploading') : t('mapPublish.upload') }}
            </n-button>
          </div>
          <p class="hint">{{ t('mapPublish.uploadHint') }}</p>
          <p v-if="expectedAssetName" class="hint">{{ t('mapPublish.assetExpected', { name: expectedAssetName }) }}</p>
        </template>

        <div class="actions">
          <n-button size="small" type="primary" :loading="savingRelease" :disabled="savingRelease" @click="submitSave">
            {{ savingRelease ? t('mapPublish.saving') : t('mapPublish.save') }}
          </n-button>
          <n-button size="small" :disabled="savingRelease" @click="closeForm">
            {{ t('mapPublish.backToList') }}
          </n-button>
        </div>
      </div>

      <div v-else class="panel">
        <div class="panel-head">
          <span v-if="selectedRepository" class="muted mono">{{ selectedRepository.full_name }}</span>
          <span v-else class="muted">{{ t('mapPublish.noSelection') }}</span>
          <n-button size="small" type="primary" :disabled="!selectedRepository || releasesLoading" @click="startNew">
            {{ t('mapPublish.newRelease') }}
          </n-button>
        </div>

        <div v-if="releasesLoading" class="state-line">
          <n-spin size="small" />
          <span>{{ t('common.loading') }}</span>
        </div>
        <p v-else-if="!releases.length" class="hint no-top">{{ t('mapPublish.releaseEmpty') }}</p>
        <ul v-else class="release-list">
          <li v-for="release in releases" :key="release.id" class="release-row">
            <div class="release-main">
              <div class="release-head">
                <code class="release-tag mono">{{ release.tag }}</code>
                <span class="release-title">{{ release.title || release.tag }}</span>
                <n-tag v-if="release.draft" size="tiny" :bordered="false">{{ t('mapPublish.draft') }}</n-tag>
                <n-tag v-if="release.prerelease" size="tiny" type="warning" :bordered="false">
                  {{ t('mapPublish.prerelease') }}
                </n-tag>
                <span class="muted">{{ release.assets.length }} {{ t('mapPublish.assets') }}</span>
              </div>
              <!-- 没有 oc2-map 块 = 这个 Release 不是地图，编辑保存后才会补上 -->
              <small v-if="!release.metadata" class="muted">{{ t('mapPublish.noMetadata') }}</small>
            </div>
            <div class="release-actions">
              <n-button size="small" :disabled="savingRelease" @click="startEdit(release)">
                {{ t('mapPublish.editRelease') }}
              </n-button>
              <n-button size="small" type="error" secondary :disabled="deletingRelease"
                @click="confirmDeleteRelease(release)">
                {{ t('mapPublish.deleteRelease') }}
              </n-button>
            </div>
          </li>
        </ul>
      </div>
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

.repo-row { display: flex; align-items: center; gap: 10px; }
.repo-select { flex: 1 1 auto; min-width: 0; max-width: 520px; }

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

.upload-row { display: flex; align-items: center; gap: 10px; margin-top: 12px; flex-wrap: wrap; }
.picked-zip {
  font-size: 12px;
  max-width: 360px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
