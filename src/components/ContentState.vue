<script setup lang="ts">
import { NEmpty, NSpin } from 'naive-ui'
import { t } from '../i18n'

withDefaults(defineProps<{
  loading?: boolean
  description?: string
  /** Page flex containers fill their remaining space; drawers stay inline. */
  layout?: 'fill' | 'inline'
}>(), { loading: false, layout: 'fill' })
</script>

<template>
  <div class="content-state" :class="`content-state--${layout}`" role="status" aria-live="polite" :aria-busy="loading || undefined">
    <template v-if="loading">
      <n-spin size="large" />
      <div class="content-state-text"><slot name="status">{{ description || t('common.loading') }}</slot></div>
    </template>
    <n-empty v-else :description="description">
      <template v-if="$slots.extra" #extra><slot name="extra" /></template>
    </n-empty>
  </div>
</template>

<style scoped>
.content-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  min-width: 0;
  min-height: 0;
  margin: 0;
  padding: 24px 0;
  text-align: center;
}
.content-state--fill { flex: 1 1 auto; }
.content-state--inline { flex: 0 0 auto; }
.content-state-text {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 2px;
  margin-top: 8px;
  font-size: 13px;
  line-height: 1.5;
  color: var(--oc2-text-2);
  pointer-events: none;
}
</style>
