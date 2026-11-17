<script setup lang="ts">
import { computed } from 'vue'
import { NButton } from 'naive-ui'
import AppModal from './AppModal.vue'
import { t } from '../i18n'
import {
  dependencyNavigationOpen, pendingDependencyNavigation,
  cancelDependencyNavigation, confirmDependencyNavigation, afterDependencyNavigationLeave,
} from '../composables/useDependencyNavigation'

const direction = computed(() => pendingDependencyNavigation.value?.jump?.versionChange?.direction ?? 'upgrade')
const title = computed(() => t(direction.value === 'downgrade' ? 'tools.downgradeTitle' : 'tools.upgradeTitle'))
const message = computed(() => t(direction.value === 'downgrade' ? 'tools.downgradeMessage' : 'tools.upgradeMessage', {
  name: pendingDependencyNavigation.value?.name ?? '',
  current: pendingDependencyNavigation.value?.jump?.versionChange?.installedVersion ?? '',
  target: pendingDependencyNavigation.value?.jump?.targetVersion ?? '',
  requirement: pendingDependencyNavigation.value?.requirement ?? '',
}))
</script>

<template>
  <AppModal v-if="pendingDependencyNavigation" :show="dependencyNavigationOpen" :title="title"
    max-width="min(520px, 94%)" card-class="dependency-navigation-modal" @click.stop
    @update:show="(show: boolean) => { if (!show) cancelDependencyNavigation() }"
    @after-leave="afterDependencyNavigationLeave">
    <p class="dependency-navigation-message">{{ message }}</p>
    <template #footer>
      <NButton @click="cancelDependencyNavigation">{{ t('common.action.cancel') }}</NButton>
      <NButton type="primary" @click="confirmDependencyNavigation">{{ t('tools.downgradeGoToMod') }}</NButton>
    </template>
  </AppModal>
</template>

<style scoped>
.dependency-navigation-message { margin: 0; line-height: 1.7; overflow-wrap: anywhere; }
</style>
