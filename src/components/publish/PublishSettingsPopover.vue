<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { NButton, NInput, NPopover, NSpin, NTag } from 'naive-ui'
import { SettingsOutline } from '@vicons/ionicons5'
import { t } from '../../i18n'
import { api } from '../../services/tauri'
import { useNotify } from '../../useNotify'
import { backendMessageOf, translateBackendMessage } from '../../locale/backend'
import type { useMapPublish } from '../../composables/useMapPublish'
const props = defineProps<{ publish: ReturnType<typeof useMapPublish>; disabled: boolean }>()
const settingsOpen = defineModel<boolean>('show', { default: false })
watch(() => props.disabled, disabled => { if (disabled) settingsOpen.value = false })
const { auth, authReady, authenticated, needsInstall, allRepositories, openInstallPage,
  signingIn, signingOut, authorization, authPhase, authFailure, repositories, repositoriesLoading,
  selectedFullName, startSignIn, startSignInAnyway, cancelSignIn,
  signOut, selectRepository } = props.publish
const message = useNotify()
const repositoryFilter = ref('')
const filteredRepositories = computed(() => repositories.value.filter(repository =>
  repository.full_name.toLowerCase().includes(repositoryFilter.value.trim().toLowerCase())))
function chooseRepository(fullName: string) {
  if (props.disabled) return
  settingsOpen.value = false
  repositoryFilter.value = ''
  void selectRepository(fullName)
}
async function reopenAuthorization() {
  const url = authorization.value?.authorize_url
  if (!url) return
  try {
    const result = await api.openReleasePage(url)
    if (!result.ok) message.error(translateBackendMessage(result.message))
  } catch (reason) { message.error(backendMessageOf(reason)) }
}
</script>

<template>
    <n-popover v-model:show="settingsOpen" trigger="click" placement="bottom-end" :show-arrow="false" raw
      :z-index="2400">
      <template #trigger>
        <n-button size="small" :disabled="disabled" :title="t('mapPublish.settings')" :aria-label="t('mapPublish.settings')">
          <template #icon><SettingsOutline width="14" height="14" /></template>
          {{ t('mapPublish.settings') }}
        </n-button>
      </template>

      <div class="map-publish publish-settings">
        <h3 class="settings-title">{{ t('mapPublish.account') }}</h3>

        <div v-if="!authReady" class="state-line">
          <n-spin size="small" />
          <span>{{ t('common.loading') }}</span>
        </div>

        <template v-else-if="authenticated">
          <div class="signed-in">
            <span class="signed-in-text">{{ t('mapPublish.signedInAs', { login: auth?.login ?? '' }) }}</span>
            <n-button size="tiny" :loading="signingOut" :disabled="signingOut || disabled" @click="signOut">
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
              :disabled="disabled || signingIn || (auth !== null && !auth.secret_configured)" @click="startSignIn">
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
              <n-button size="tiny" type="primary" :loading="signingIn" :disabled="signingIn || disabled"
                @click="startSignIn">
                {{ t('mapPublish.installAppNow') }}
              </n-button>
            </div>
          </div>

          <template v-else>
            <div class="repo-tools">
              <n-input v-model:value="repositoryFilter" size="tiny" clearable
                :placeholder="t('mapPublish.searchRepository')" />
            </div>

            <div v-if="repositoriesLoading" class="state-line">
              <n-spin size="small" />
              <span>{{ t('common.loading') }}</span>
            </div>
            <ul v-else-if="filteredRepositories.length" class="repo-list">
              <li v-for="repository in filteredRepositories" :key="repository.full_name">
                <button type="button" class="repo-item"
                  :class="{ active: repository.full_name === selectedFullName }"
                  :disabled="disabled" @click="chooseRepository(repository.full_name)">
                  <span class="repo-name mono">{{ repository.full_name }}</span>
                  <n-tag v-if="repository.topics.includes('map')" size="tiny" type="success" :bordered="false">
                    map
                  </n-tag>
                </button>
              </li>
            </ul>
            <p v-else-if="repositories.length" class="hint no-top">{{ t('mapPublish.searchRepositoryEmpty') }}</p>
            <p v-else class="hint no-top">{{ t('mapPublish.repositoryEmpty') }}</p>

            <p class="hint no-top">{{ t('mapPublish.repositoryHint') }}</p>
            <p v-if="!allRepositories" class="hint no-top">{{ t('mapPublish.repositoryScopeHint') }}</p>

            <p class="hint no-top">{{ t('mapPublish.manualRepositoryHint') }}</p>
          </template>
        </template>
      </div>
    </n-popover>
</template>

<style scoped>
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

</style>
