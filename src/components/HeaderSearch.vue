<script setup lang="ts">
import { NInput } from 'naive-ui'
import { SearchOutline } from '@vicons/ionicons5'
import { t } from '../i18n'

const value = defineModel<string>('value', { required: true })
</script>

<template>
  <n-input v-model:value="value" :clearable="value.length > 0" size="small"
    class="header-search"
    :placeholder="t('dialogs.search')" :input-props="{ 'aria-label': t('dialogs.search') }">
    <template #prefix><SearchOutline width="14" height="14" aria-hidden="true" /></template>
  </n-input>
</template>

<style scoped>
/* 输入法组合期间保持输入区域宽度稳定，避免文字镜像挤压原生光标。 */
.header-search { width: min(280px, 32vw); flex: 0 1 auto; min-width: 120px; }
</style>
