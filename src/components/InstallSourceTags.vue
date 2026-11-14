<script setup lang="ts">
import { computed } from 'vue'
import { NTag } from 'naive-ui'
import { useDashboard } from '../composables/useDashboardStore'
import { t } from '../i18n'
import { dataProviderShortLabel } from '../providers'
import { repositoryRegionLabel } from '../repositorySources'

const { config } = useDashboard()
const provider = computed(() => config.value?.data_provider)
const region = computed(() => config.value?.region)
</script>

<template>
  <n-tag v-if="region" size="small" :bordered="false" type="info" class="install-source-tag" data-testid="install-region">
    {{ repositoryRegionLabel(region) }}
  </n-tag>
  <n-tag v-if="provider" size="small" :bordered="false" class="install-source-tag" :data-provider="provider">
    {{ dataProviderShortLabel(provider, t('settings.dataProviderBuiltinShort')) }}
  </n-tag>
</template>

<style scoped>
.install-source-tag { flex: 0 0 auto; }
</style>
