<script setup lang="ts">
import { t } from '../i18n'
import { computed, onMounted, onUnmounted, shallowRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import { api, watchAppUpdatePhase } from '../services/tauri'
import AppModal from './AppModal.vue'
import MarkdownBody from './MarkdownBody.vue'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { beginDownloadTask, endDownloadTask } from '../downloadTask'
import {
  appUpdateBusy, appUpdateState, beginAppUpdate, beginBackgroundAppUpdate,
  finishAppUpdate, finishBackgroundAppUpdate, setBackgroundAppUpdateProgress,
} from '../appUpdateState'

const update = computed(() => appUpdateState.update)
const show = computed(() => ['required', 'installing', 'manual', 'stageFailed'].includes(appUpdateState.stage)
  || appUpdateState.stage === 'handoff' && !update.value?.background_install)
const installing = computed(() => appUpdateState.stage === 'installing')
const busy = computed(appUpdateBusy)
const manual = computed(() => appUpdateState.stage === 'manual')
const stageFailed = computed(() => appUpdateState.stage === 'stageFailed')
const manualRequired = computed(() => manual.value && appUpdateState.error !== 'update.deferredUnsupported')
const failureDetail = computed(() => {
  const text = translateBackendMessage(appUpdateState.error)
  return manual.value && text === t('update.manualRequired') ? '' : text
})
const phaseText = shallowRef('')
const openingRelease = shallowRef(false)
const releaseError = shallowRef('')

let disposed = false
const listeners: (() => void)[] = []
function keepListener(unlisten: () => void) {
  if (disposed) unlisten()
  else listeners.push(unlisten)
}
void watchAppUpdatePhase((event) => {
  if (installing.value && event.message) phaseText.value = event.message
}).then(keepListener).catch(() => undefined)

function preventReload(event: BeforeUnloadEvent) {
  if (!busy.value) return
  event.preventDefault()
  event.returnValue = ''
}
onMounted(() => {
  window.addEventListener('beforeunload', preventReload)
})
onUnmounted(() => {
  disposed = true
  listeners.forEach(unlisten => unlisten())
  window.removeEventListener('beforeunload', preventReload)
})

const bodyText = computed(() => update.value?.body?.trim() ?? '')

async function stageUpdate() {
  const channel = beginBackgroundAppUpdate()
  if (!channel) return
  releaseError.value = ''
  try {
    await api.stageAppUpdate(channel, setBackgroundAppUpdateProgress)
    finishBackgroundAppUpdate()
  } catch (reason) {
    const error = reason instanceof Error ? reason.message : typeof reason === 'string' ? reason : 'update.stageFailed'
    finishBackgroundAppUpdate(error || 'update.stageFailed')
  }
}

watch(() => appUpdateState.stage, stage => {
  if (stage === 'backgroundRequired') void stageUpdate()
}, { immediate: true })

async function install() {
  if (!beginAppUpdate()) return
  phaseText.value = t('dialogs.preparingDownload')
  // 最终失败由强制更新弹窗接管，不产生可关闭/重试的普通下载失败卡。
  const token = beginDownloadTask({
    label: update.value ? `v${update.value.version}` : t('dialogs.update'),
    retry: install,
  })
  try {
    const result = await api.installAppUpdate()
    finishAppUpdate(result.ok, result.message)
    phaseText.value = result.ok ? (result.message || 'update.installerStarted') : ''
  } catch (reason) {
    finishAppUpdate(false, backendMessageOf(reason))
    phaseText.value = ''
  } finally {
    endDownloadTask(token)
  }
}

async function openRelease() {
  if ((!manual.value && !stageFailed.value) || openingRelease.value) return
  openingRelease.value = true
  releaseError.value = ''
  try {
    const result = await api.openUpdateRepository()
    if (!result.ok) releaseError.value = translateBackendMessage(result.message)
  } catch (reason) {
    releaseError.value = backendMessageOf(reason)
  } finally {
    openingRelease.value = false
  }
}
</script>

<template>
  <AppModal :show="show" :closable="false" :mask-closable="false"
    :z-index="6100" card-class="mandatory-update" max-width="min(520px, 92%)"
    max-height="85%" min-height="160px">
    <div class="update-body">
      <div class="update-heading">
        <h2 class="title">{{ t('dialogs.versionAvailable', { version: 'v' + update?.version }) }}</h2>
        <span v-if="update" class="version-line muted">v{{ update.current_version }} → v{{ update.version }}</span>
      </div>
      <p v-if="manualRequired" class="update-requirement">{{ t('update.manualRequired') }}</p>
      <div v-if="(manual || stageFailed) && failureDetail" class="update-error" role="alert">
        {{ failureDetail }}
      </div>
      <div v-if="releaseError" class="update-error" role="alert">{{ releaseError }}</div>
      <MarkdownBody v-if="bodyText" class="body" :source="bodyText" />
      <div v-else class="body">{{ t('dialogs.noReleaseNotesWereProvided') }}</div>
      <div v-if="phaseText" class="phase" role="status">{{ translateBackendMessage(phaseText) }}</div>
    </div>
    <template #footer>
      <n-button v-if="manual || stageFailed" :type="manual ? 'primary' : 'default'"
        :loading="openingRelease" @click="openRelease">
        {{ t('dialogs.openReleasePage') }}
      </n-button>
      <n-button v-if="stageFailed" type="primary" :disabled="openingRelease" @click="stageUpdate">
        {{ t('update.retryDownload') }}
      </n-button>
      <n-button v-else-if="!manual" type="primary" :loading="busy" :disabled="busy" @click="install">
        {{ t('dialogs.update') }}
      </n-button>
    </template>
  </AppModal>
</template>

<style scoped>
/* 外壳（卡片边框/内边距/滚动）由 AppModal 负责，这里只管内容排版 */
.update-body {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.update-requirement { margin: 0 0 8px; color: var(--oc2-text-2); }
.update-error { color: var(--oc2-accent); overflow-wrap: anywhere; }
.update-heading {
  display: flex;
  align-items: baseline;
  flex-wrap: wrap;
  gap: 4px 12px;
  margin-bottom: 6px;
}
.title {
  margin: 0;
  font-size: 21px;
  font-weight: 700;
}
.version-line { font-size: 13px; white-space: nowrap; }
/*
 * 正文不再自己限高滚动：外层 AppModal 已经统一滚动，
 * 这里再套一层会出现两级滚动，手势体验很差。
 */
.body {
  white-space: pre-wrap;
  padding: 10px 12px;
  border: 1px solid var(--oc2-divider);
  border-radius: 8px;
  background: var(--oc2-sunken);
  font-size: 13px;
  line-height: 1.6;
  color: var(--oc2-text-2);
}
/* Markdown 渲染时交给块级排版，不要再叠加 pre-wrap。 */
.body.markdown-body {
  white-space: normal;
}
.phase {
  margin-top: 10px;
  color: var(--oc2-accent);
  font-size: 12.5px;
}
</style>
