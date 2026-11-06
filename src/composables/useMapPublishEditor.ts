import { computed, nextTick, onBeforeUnmount, reactive, ref, watch, type Ref } from 'vue'
import { api } from '../services/tauri'
import { t } from '../i18n'
import { useNotify } from '../useNotify'
import { backendMessageOf } from '../locale/backend'
import { composeAssetRule, renderAssetName } from '../mapPublishAsset'
import { defaultMapTag, stepAvailable, stepIssues, publishZipIssue, type PublishZipIssue, type PublishStepId, type AssetVersionRow } from '../mapPublishFlow'
import { assetNameForRow, metadataForStep, reconcileVersionRows, releaseAssetRule, unregisteredReleaseVersions, versionRowsOf } from '../mapPublishRelease'
import { useMapPublishDraft } from './useMapPublishDraft'
import { usePublishValidation } from './usePublishValidation'
import { usePublishLeaveGuard } from './usePublishLeaveGuard'
import type { useMapPublish } from './useMapPublish'
import type { MapPublishMetadata, PublishRelease, PublishZipInspection } from '../types'

/** 本地准备和上传共用草稿；非上传场景的日志、资料、版本保持独立保存。 */
export function useMapPublishEditor(publish: ReturnType<typeof useMapPublish>, root: Ref<HTMLElement | null>) {
  const message = useNotify()
  const document = useMapPublishDraft()
  const validation = usePublishValidation(root)
  const mode = ref<'list' | 'create' | 'manage'>('list')
  const activeTag = ref('')
  const activeStep = ref<PublishStepId>('release')
  const stepBusy = ref<PublishStepId | null>(null)
  const form = reactive({ tag: '', body: '' })
  const selectingZip = ref(false)
  let tagEdited = false
  const setTag = (value: string) => { form.tag = value; tagEdited = true }
  const repositoryName = computed(() => publish.selectedRepository.value?.full_name ?? '')
  const assetRows = ref<AssetVersionRow[]>([])
  const metadataDirty = ref(false)
  const versionsDirty = ref(false)
  const pendingUpload = ref<{ id: number; name: string; owner: string; repository: string } | null>(null)
  const awaitingMetadata = computed(() => pendingUpload.value !== null)
  const pendingIdentity = ref<PublishZipInspection | MapPublishMetadata | null>(null)
  const pending = reactive({
    path: '', fileName: '', version: '', inspection: null as PublishZipInspection | null,
    checking: false, accepted: false, issue: null as PublishZipIssue | null,
  })
  const inspectionError = computed(() => {
    switch (pending.issue) {
      case 'multiple': return t('mapPublish.inspectionMultiple', { count: pending.inspection?.set_count ?? 0 })
      case 'version': return t('mapPublish.missingVersion')
      case 'identity': return t('mapPublish.zipIdentityMismatch')
      default: return ''
    }
  })
  let editorEpoch = 0
  let applying = false
  const workingRelease = computed(() => publish.releases.value.find(item => item.tag === activeTag.value) ?? null)
  const existingRule = computed(() => workingRelease.value?.metadata?.asset.trim() ?? '')
  const effectiveRule = computed(() => workingRelease.value ? releaseAssetRule(workingRelease.value) : composeAssetRule(form.tag.trim()))
  const canonicalName = computed(() => renderAssetName(effectiveRule.value, '', pending.version))
  const latestRow = computed(() => assetRows.value.at(-1) ?? null)
  const unregistered = computed(() => workingRelease.value ? unregisteredReleaseVersions(workingRelease.value) : [])
  const operationBusy = computed(() => stepBusy.value !== null || publish.savingRelease.value ||
    publish.uploading.value || publish.inspecting.value || pending.checking || publish.deletingAsset.value || publish.deletingRelease.value || publish.repositoriesLoading.value || publish.releasesLoading.value)
  const preparing = computed(() => mode.value !== 'list' && (Boolean(pending.path) || awaitingMetadata.value || !workingRelease.value?.metadata))
  const localDirty = computed(() => mode.value !== 'list' && (Boolean(pending.path) || metadataDirty.value || versionsDirty.value ||
    form.body !== (workingRelease.value?.body ?? '') || (!workingRelease.value && Boolean(form.tag))))
  const leave = usePublishLeaveGuard(operationBusy, computed(() => pendingUpload.value?.name ?? ''), async () => {
    const uploaded = pendingUpload.value
    if (!uploaded) return true
    if (!await publish.deleteAsset(uploaded.id, uploaded)) return false
    pendingUpload.value = null
    return true
  }, localDirty)
  const busyAny = computed(() => operationBusy.value || leave.confirming.value)
  const canSelectStep = (step: PublishStepId) => stepAvailable(step, Boolean(workingRelease.value), awaitingMetadata.value, pending.accepted)
  watch(() => [document.draft.levelSetName, document.draft.levelSetNameZh, document.draft.author,
    document.draft.nameI18n, document.draft.groupI18n, document.draft.noteI18n,
    document.draft.requires, document.draft.compatible, document.draft.conflicts], () => {
    if (!applying) metadataDirty.value = true
  }, { deep: true, flush: 'sync' })
  watch(() => [assetRows.value, document.draft.releaseRelations], () => {
    if (!applying) versionsDirty.value = true
  }, { deep: true, flush: 'sync' })
  watch(workingRelease, (release, before) => {
    if (release && before?.id === release.id) {
      const rows = reconcileVersionRows(assetRows.value, release)
      if (JSON.stringify(rows) !== JSON.stringify(assetRows.value)) assetRows.value = rows
    }
  }, { flush: 'sync' })

  function rejectPending() {
    Object.assign(pending, { path: '', fileName: '', version: '', inspection: null, checking: false, accepted: false, issue: null })
  }
  function resetForm(release: PublishRelease | null) {
    editorEpoch += 1
    validation.clear()
    validation.metadataSection.value = 'i18n'
    applying = true
    try {
      tagEdited = false
      selectingZip.value = false
      form.tag = release?.tag ?? ''
      form.body = release?.body ?? ''
      document.reset(release?.metadata ?? null)
      assetRows.value = versionRowsOf(release)
      pendingIdentity.value = release?.metadata ?? null
      rejectPending()
      metadataDirty.value = versionsDirty.value = false
    } finally { applying = false }
  }
  function openManagement(release: PublishRelease) {
    if (busyAny.value || awaitingMetadata.value) return
    activeTag.value = release.tag
    resetForm(release)
    mode.value = 'manage'
    activeStep.value = 'versions'
  }
  function openCreate() {
    if (busyAny.value || awaitingMetadata.value || !publish.selectedRepository.value) return
    activeTag.value = ''
    resetForm(null)
    mode.value = 'create'
    selectingZip.value = true
    activeStep.value = 'release'
  }
  function resetToList() {
    activeTag.value = ''
    resetForm(null)
    mode.value = 'list'
    activeStep.value = 'release'
  }
  async function backToList() {
    if (await leave.requestLeave()) resetToList()
  }
  watch(publish.selectedFullName, () => { if (!awaitingMetadata.value) resetToList() }, { flush: 'sync' })
  onBeforeUnmount(() => { editorEpoch += 1 })
  function selectStep(step: PublishStepId) {
    if (!busyAny.value && canSelectStep(step)) {
      validation.clear()
      selectingZip.value = false
      activeStep.value = step
    }
  }

  async function validate(step: PublishStepId): Promise<boolean> {
    if (step === 'asset' && !pending.accepted) {
      selectingZip.value = true
      if (inspectionError.value) message.error(inspectionError.value)
      await validation.pulse('zip')
      return false
    }
    const issues = stepIssues(step, {
      tag: form.tag, releaseExists: Boolean(workingRelease.value),
      assetName: pending.path && pending.inspection ? canonicalName.value : '',
      versionText: pending.version, levelSetName: document.draft.levelSetName, versionCount: assetRows.value.length,
    })
    // 历史 tag 已经存在且只读，不以新建规则阻止修改发布说明。
    if (step === 'release' && workingRelease.value) return true
    if (issues.includes('release')) return false
    if (issues.includes('levelSetName') || issues.includes('versionRows')) {
      selectingZip.value = !document.draft.levelSetName
      activeStep.value = 'asset'
      message.warning(t('mapPublish.needZip'))
      await validation.pulse('zip')
      return false
    }
    if (issues.length) {
      activeStep.value = issues[0] === 'tag' ? 'release' : 'asset'
      await validation.pulse(issues[0] === 'tag' ? 'tag' : 'zip')
      return false
    }
    if (step === 'release' && publish.releases.value.some(item => item.tag === form.tag.trim())) {
      activeStep.value = 'release'
      message.warning(t('mapPublish.tagExists', { tag: form.tag.trim() }))
      await validation.pulse('tag')
      return false
    }
    if (step === 'metadata' && document.missingFields.value.length) {
      activeStep.value = 'metadata'
      await validation.pulse(document.missingFields.value[0]!)
      return false
    }
    if (step === 'metadata' && document.invalidRelation.value) {
      activeStep.value = 'metadata'
      message.warning(t('mapPublish.invalidRelations'))
      await validation.pulse(document.invalidRelation.value)
      return false
    }
    if (step === 'versions' && document.invalidVersionRelation.value) {
      message.warning(t('mapPublish.invalidRelations'))
      await validation.pulse(document.invalidVersionRelation.value)
      return false
    }
    return true
  }

  async function loadPendingZip(path: string) {
    if (busyAny.value || awaitingMetadata.value || !publish.selectedRepository.value) return
    if (mode.value === 'list') openCreate()
    selectingZip.value = true
    activeStep.value = 'asset'
    rejectPending()
    const epoch = editorEpoch
    const result = await publish.inspectZip(path)
    if (epoch !== editorEpoch || !result) return
    const version = result.info_version.trim()
    Object.assign(pending, { path, fileName: result.file_name || path.split(/[\\/]/).pop() || path, version, inspection: result, checking: true })
    // 先让只读表格落进 DOM，失败的 ZIP 也必须留下信息供作者核对。
    await nextTick()
    if (epoch !== editorEpoch) return
    const locked = workingRelease.value?.metadata ?? (workingRelease.value ? pendingIdentity.value : null)
    pending.issue = publishZipIssue(result, locked)
    pending.checking = false
    if (pending.issue) return
    pending.accepted = true
    pendingIdentity.value = locked ?? result
    document.applyInspection(result, !workingRelease.value?.metadata)
    if (!workingRelease.value && !tagEdited) {
      const suggested = defaultMapTag(result.level_set_name)
      form.tag = publish.releases.value.some(item => item.tag === suggested) ? '' : suggested
      if (suggested && !form.tag) message.warning(t('mapPublish.tagExists', { tag: suggested }))
    }
    selectingZip.value = false
    activeStep.value = 'release'
  }
  async function pickPendingZip() {
    if (busyAny.value || awaitingMetadata.value) return
    const epoch = editorEpoch
    try {
      const path = await api.chooseLevelSetZip()
      if (path && epoch === editorEpoch) await loadPendingZip(path)
    } catch (reason) { message.error(backendMessageOf(reason)) }
  }

  async function saveReleaseStep(): Promise<boolean> {
    if (busyAny.value || !await validate('release') || busyAny.value) return false
    if (preparing.value) { activeStep.value = 'metadata'; return true }
    const epoch = editorEpoch
    const release = workingRelease.value
    if (!release) return false
    stepBusy.value = 'release'
    try {
      if (form.body !== release.body) {
        const saved = await publish.saveRelease({ releaseId: release.id, tag: release.tag, body: form.body })
        if (!saved || epoch !== editorEpoch) return false
        form.body = saved.body
      }
      return true
    } finally { stepBusy.value = null }
  }

  async function uploadPending(): Promise<boolean> {
    if (awaitingMetadata.value || busyAny.value || !await validate('release') || !await validate('metadata') || !await validate('asset') || busyAny.value) return false
    let release = workingRelease.value
    const repository = publish.selectedRepository.value
    if (!repository || !pending.inspection) return false
    const name = canonicalName.value
    if (release && release.assets.some(asset => asset.name === name)) {
      message.warning(t('mapPublish.assetExists', { name }))
      return false
    }
    const epoch = editorEpoch
    const version = pending.version
    const infoVersion = pending.inspection.info_version.trim()
    stepBusy.value = 'asset'
    try {
      if (!release) {
        // 创建成功即固定 tag；上传失败重试复用此 Release，正文草稿留到最终登记。
        release = await publish.saveRelease({ releaseId: null, tag: form.tag.trim(), body: '' }, { quiet: true })
        if (!release || epoch !== editorEpoch) return false
        activeTag.value = release.tag
        form.tag = release.tag
      }
      const asset = await publish.uploadAsset(release, pending.path)
      if (!asset || epoch !== editorEpoch) return false
      pendingUpload.value = { id: asset.id, name: asset.name, owner: repository.owner, repository: repository.name }
      const row = assetRows.value.find(item => item.mapsetVersion === version)
      if (row) row.infoVersion = infoVersion
      else assetRows.value.push({ mapsetVersion: version, infoVersion, size: asset.size })
      rejectPending()
      activeStep.value = 'versions'
      return true
    } finally { stepBusy.value = null }
  }

  function buildMetadata(): MapPublishMetadata {
    return {
      ...document.buildFields(), asset: existingRule.value,
      latest_mapset_version: latestRow.value?.mapsetVersion.trim() ?? '',
      latest_info_version: latestRow.value?.infoVersion.trim() ?? '',
      asset_versions: assetRows.value.filter(row => row.mapsetVersion.trim()).map(row => ({
        mapset_version: row.mapsetVersion.trim(), info_version: row.infoVersion.trim(),
      })),
    }
  }
  async function saveDocument(step: 'metadata' | 'versions' = 'metadata'): Promise<boolean> {
    if (busyAny.value || !await validate(step) || busyAny.value) return false
    if (step === 'metadata' && preparing.value) {
      activeStep.value = awaitingMetadata.value ? 'versions' : 'asset'
      return true
    }
    const release = workingRelease.value
    if (!release) return false
    const complete = step === 'versions' && (awaitingMetadata.value || !release.metadata)
    // 上传流程统一登记全部草稿；日常直接维护仍隔离三个保存域。
    if (complete && (!await validate('release') || !await validate('metadata'))) return false
    const epoch = editorEpoch
    stepBusy.value = step
    try {
      const saved = await publish.saveRelease({ releaseId: release.id, tag: release.tag, body: complete ? form.body : release.body,
        metadata: complete ? buildMetadata() : metadataForStep(step, release.metadata, buildMetadata()) })
      if (!saved || epoch !== editorEpoch) return false
      if (step === 'versions') {
        pendingUpload.value = null
        versionsDirty.value = false
        if (complete) {
          metadataDirty.value = false
          form.body = saved.body
          rejectPending()
        }
      } else metadataDirty.value = false
      activeStep.value = 'versions'
      return true
    } finally { stepBusy.value = null }
  }
  const assetNameOf = (row: AssetVersionRow) => workingRelease.value ? assetNameForRow(workingRelease.value, row) : ''
  return {
    mode, activeTag, activeStep, stepBusy, form, document, assetRows, metadataDirty, versionsDirty, awaitingMetadata, pending,
    selectingZip, preparing, repositoryName, setTag,
    workingRelease, canonicalName, inspectionError, unregistered, busyAny, validation, canSelectStep,
    openManagement, openCreate, backToList, selectStep, loadPendingZip, pickPendingZip,
    saveReleaseStep, uploadPending, saveDocument, assetNameOf,
  }
}
export type MapPublishEditor = ReturnType<typeof useMapPublishEditor>
