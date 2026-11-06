<script setup lang="ts">
import { computed, h, onMounted, ref } from 'vue'
import { NButton, NCheckbox, NSpin, NTag, useDialog } from 'naive-ui'
import { RefreshOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { api } from '../services/tauri'
import PageLoading from '../components/PageLoading.vue'
import PublishSettingsPopover from '../components/publish/PublishSettingsPopover.vue'
import PublishReleaseList from '../components/publish/PublishReleaseList.vue'
import PublishReleaseStep from '../components/publish/PublishReleaseStep.vue'
import PublishAssetStep from '../components/publish/PublishAssetStep.vue'
import PublishMetadataStep from '../components/publish/PublishMetadataStep.vue'
import { useMapPublish } from '../composables/useMapPublish'
import { useMapPublishEditor } from '../composables/useMapPublishEditor'
import { useFileDrop } from '../composables/useFileDrop'
import { displayName } from '../mapPublishRelease'
import { PUBLISH_STEP_ORDER, permissionScopeKey, type PublishStepId } from '../mapPublishFlow'
import { markDialogBusy } from '../dialogBusy'
import { useNotify } from '../useNotify'
import { backendMessageOf, translateBackendMessage } from '../locale/backend'
import type { PublishRelease } from '../types'
import '../styles/publish.css'

/** 发布信息 → 上传 ZIP → 地图资料分别保存；已有地图默认上传，新建从发布信息开始。 */
const message = useNotify()
const dialog = useDialog()
const publish = useMapPublish()
const { auth, authReady, authLoading, authenticated, needsInstall, openInstallPage,
  signingIn, authorization, authPhase, authFailure, repositories, repositoriesLoading,
  selectedRepository, releases, releasesLoading, deletingRelease, inspecting, uploadProgress,
  permissionFailure, missingTopics, loadState, startSignIn, startSignInAnyway,
  cancelSignIn, refreshRepositories, deleteRelease, deleteAsset } = publish
const editorRoot = ref<HTMLElement | null>(null)
const editor = useMapPublishEditor(publish, editorRoot)
const { mode, activeTag, activeStep, form, workingRelease, latestRow, unregistered, busyAny, awaitingMetadata, canSelectStep,
  openCreate, openManagement, backToList, selectStep, loadPendingZip } = editor
const settingsOpen = ref(false)
const openSettings = () => { settingsOpen.value = true }
onMounted(loadState)
const identityTitle = computed(() => workingRelease.value ? displayName(workingRelease.value) : form.tag.trim())
const identityShowsTitle = computed(() => Boolean(identityTitle.value && identityTitle.value !== form.tag.trim()))
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

const { dragActive } = useFileDrop({
  accept: ['zip'], hint: () => t('mapPublish.dropToInspect'), busy: () => busyAny.value || awaitingMetadata.value,
  onDrop: path => { void loadPendingZip(path) },
})
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
  if (busyAny.value || awaitingMetadata.value) return
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
    <n-button size="small" :loading="repositoriesLoading || releasesLoading" :disabled="!authenticated || busyAny || awaitingMetadata"
      @click="refreshRepositories">
      <template #icon><RefreshOutline width="14" height="14" /></template>
      {{ t('common.action.refresh') }}
    </n-button>
    <PublishSettingsPopover v-model:show="settingsOpen" :publish="publish" :disabled="busyAny || awaitingMetadata" />
  </Teleport>
  <div ref="editorRoot" class="map-publish">
  <PageLoading :show="authLoading">
    <template #status>{{ t('common.loading') }}</template>

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

      <PublishReleaseList v-if="mode === 'list'" :repository-name="selectedRepository.full_name"
        :releases="releases" :releases-loading="releasesLoading" :deleting-release="deletingRelease"
        @create="openCreate" @open="openManagement" @delete="confirmDeleteRelease" />
      <template v-else>
        <!-- 身份固定；三个步骤分别保存。 -->
        <div class="panel publish-head">
          <div class="panel-head">
            <div class="manage-head">
              <code class="release-tag mono">
                {{ workingRelease?.tag || form.tag.trim() || t('mapPublish.newMapSet') }}
              </code>
              <span v-if="identityShowsTitle" class="release-title">{{ identityTitle }}</span>
              <n-tag size="tiny" :bordered="false">
                {{ workingRelease ? t('mapPublish.mapManagement') : t('mapPublish.newMapSet') }}
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
          </div>
        </div>

        <p v-if="awaitingMetadata" class="warn">{{ t('mapPublish.uploadNeedsSave') }}</p>
        <p v-else-if="unregistered.length" class="warn">
          {{ t('mapPublish.unregisteredVersions', { count: unregistered.length }) }}
        </p>
        <nav class="publish-step-nav" :aria-label="t('mapPublish.title')">
          <n-button v-for="(step, index) in PUBLISH_STEP_ORDER" :key="step" size="small"
            :type="activeStep === step ? 'primary' : 'default'" :aria-current="activeStep === step ? 'step' : undefined"
            :disabled="busyAny || !canSelectStep(step)" @click="selectStep(step)">
            {{ index + 1 }}. {{ stepTitle(step) }}
          </n-button>
        </nav>
        <PublishReleaseStep v-if="activeStep === 'release'" :editor="editor" />
        <PublishAssetStep v-else-if="activeStep === 'asset'" :editor="editor" :drag-active="dragActive"
          :inspecting="inspecting" :upload-progress="uploadProgress" />
        <PublishMetadataStep v-else :editor="editor" @remove="confirmDeleteAsset" />
      </template>
    </section>
  </PageLoading>
  </div>
</template>
