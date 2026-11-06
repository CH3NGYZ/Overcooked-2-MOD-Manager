<script setup lang="ts">
import { t } from '../i18n'
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { NButton } from 'naive-ui'
import { api, watchAppUpdatePhase } from '../services/tauri'
import { renderMermaidBlocks, renderReadme } from '../markdown'
import AppModal from './AppModal.vue'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import { beginDownloadTask, endDownloadTask } from '../downloadTask'
import { appUpdateState, beginAppUpdate, finishAppUpdate } from '../appUpdateState'

const update = computed(() => appUpdateState.update)
const show = computed(() => appUpdateState.stage !== 'idle')
const installing = computed(() => appUpdateState.stage === 'installing')
const busy = computed(() => installing.value || appUpdateState.stage === 'handoff')
const manual = computed(() => appUpdateState.stage === 'manual')
const failureDetail = computed(() => {
  const text = translateBackendMessage(appUpdateState.error)
  return text === t('update.manualRequired') ? '' : text
})
const phaseText = ref('')
const openingRelease = ref(false)
const releaseError = ref('')

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

const bodyText = computed(() => update.value?.body?.trim() || t('dialogs.noReleaseNotesWereProvided'))
const bodyHtml = computed(() => renderReadme(update.value?.body?.trim() || ''))
const bodyElement = ref<HTMLElement | null>(null)
watch([bodyHtml, show], async () => {
  await nextTick()
  if (show.value) void renderMermaidBlocks(bodyElement.value)
}, { flush: 'post' })

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
  if (!manual.value || openingRelease.value) return
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
      <h2 class="title">{{ t('dialogs.versionAvailable', { version: 'v' + update?.version }) }}</h2>
      <div v-if="update" class="version-line muted">v{{ update.current_version }} → v{{ update.version }}</div>
      <p class="update-requirement">{{ t(manual ? 'update.manualRequired' : 'update.required') }}</p>
      <div v-if="manual && failureDetail" class="update-error" role="alert">
        {{ failureDetail }}
      </div>
      <div v-if="releaseError" class="update-error" role="alert">{{ releaseError }}</div>
      <div class="body-label muted">{{ t('dialogs.whatSNew') }}</div>
      <div v-if="bodyHtml" ref="bodyElement" class="body markdown-body" v-html="bodyHtml"></div>
      <div v-else class="body">{{ bodyText }}</div>
      <div v-if="phaseText" class="phase" role="status">{{ translateBackendMessage(phaseText) }}</div>
    </div>
    <template #footer>
      <n-button v-if="manual" type="primary" :loading="openingRelease" @click="openRelease">
        {{ t('dialogs.openReleasePage') }}
      </n-button>
      <n-button v-else type="primary" :loading="busy" :disabled="busy" @click="install">
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
.title {
  margin: 0 0 6px;
  font-size: 21px;
  font-weight: 700;
}
.version-line { margin-bottom: 12px; }
.body-label { margin-bottom: 6px; }
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
  overflow-wrap: anywhere;
}
.body.markdown-body :deep(h1),
.body.markdown-body :deep(h2),
.body.markdown-body :deep(h3) {
  margin: 12px 0 6px;
  font-size: 14px;
  color: var(--oc2-text-1);
}
.body.markdown-body :deep(p) { margin: 0 0 8px; }
.body.markdown-body :deep(ul),
.body.markdown-body :deep(ol) { margin: 0 0 8px; padding-left: 20px; }
.body.markdown-body :deep(code) {
  padding: 1px 4px;
  border-radius: 4px;
  background: var(--oc2-row-line);
  font-family: Consolas, monospace;
  font-size: 12px;
}
.body.markdown-body :deep(pre) {
  margin: 0 0 8px;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--oc2-card);
  overflow-x: auto;
}
.body.markdown-body :deep(pre code) { padding: 0; background: none; }
.body.markdown-body :deep(a) { color: var(--oc2-accent); }
.phase {
  margin-top: 10px;
  color: var(--oc2-accent);
  font-size: 12.5px;
}
</style>
