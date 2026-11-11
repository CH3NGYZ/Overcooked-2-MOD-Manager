<script setup lang="ts">
import { computed } from 'vue'
import { NButton } from 'naive-ui'
import { t } from '../i18n'
import { useDashboard } from '../composables/useDashboardStore'
import { useModRepositoryLink } from '../crossLink'
import { modRepositoryForGuid } from '../repositoryAvailability'
import ContentState from './ContentState.vue'

const props = withDefaults(defineProps<{
  guid: string
  name: string
  layout?: 'fill' | 'inline'
  disabled?: boolean
}>(), { disabled: false })
const { config } = useDashboard()
const { linkToModRepository } = useModRepositoryLink()
const repository = computed(() => modRepositoryForGuid(props.guid, config.value))

function openInstall() {
  if (!props.disabled && repository.value) linkToModRepository(repository.value.id)
}
</script>

<template>
  <ContentState :layout="layout" :description="t('modsLocal.installBeforeManaging', { name })" :data-mod-not-installed="guid">
    <template v-if="repository" #extra>
      <n-button size="small" type="primary" :disabled="disabled" data-open-mod-install @click="openInstall">{{ t('modsOnline.goToInstall') }}</n-button>
    </template>
  </ContentState>
</template>
