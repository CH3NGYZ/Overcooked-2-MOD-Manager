<script setup lang="ts">
import { NButton } from 'naive-ui'
import { DownloadOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { buttonColorFor } from '../theme'

const emit = defineEmits<{ repair: [] }>()
</script>

<template>
  <n-button size="tiny" :style="buttonColorFor('critical', 'soft')"
    :title="t('modChanges.repairDependencies')" @click.stop="emit('repair')">
    <DownloadOutline width="14" height="14" />
    {{ t('modChanges.repairDependencies') }}
  </n-button>
</template>
