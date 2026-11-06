<script setup lang="ts">
import { NInput } from 'naive-ui'
import { SearchOutline } from '@vicons/ionicons5'
import { t } from '../i18n'

const value = defineModel<string>('value', { required: true })
</script>

<template>
  <n-input v-model:value="value" autosize :clearable="value.length > 0" size="small"
    class="header-search" :class="{ 'header-search--empty': !value }"
    :placeholder="t('dialogs.search')" :input-props="{ 'aria-label': t('dialogs.search') }">
    <template #prefix><SearchOutline width="14" height="14" aria-hidden="true" /></template>
  </n-input>
</template>

<style scoped>
.header-search { width: auto; flex: 0 1 auto; min-width: 0; max-width: 240px; }
/* 空值时用译文参与布局，输入后交回 Naive 的文字镜像；不预留清除按钮空位。 */
.header-search--empty :deep(.n-input__placeholder) { position: static; height: var(--n-height); }
.header-search--empty :deep(.n-input__input-mirror) { display: none; }
</style>
