<script setup lang="ts">
import { shallowRef } from 'vue'
import { NButton, NTooltip } from 'naive-ui'

defineOptions({ inheritAttrs: false })
defineProps<{ label: string; description?: string }>()
const focused = shallowRef(false)
</script>

<template>
  <n-tooltip placement="top" :delay="150" :keep-alive-on-hover="false" :show="focused ? true : undefined">
    <template #trigger>
      <!-- 外壳接收悬停，让禁用中的按钮仍能说明用途；点击只交给原按钮。 -->
      <span class="header-icon-action" @focusin="focused = true" @focusout="focused = false">
        <n-button v-bind="$attrs" size="small" :aria-label="label" :aria-description="description">
          <template #icon><slot /></template>
        </n-button>
      </span>
    </template>
    {{ label }}
  </n-tooltip>
</template>

<style scoped>
.header-icon-action { display: inline-flex; flex: 0 0 auto; }
.header-icon-action :deep(.n-button) { width: 32px; padding: 0; }
</style>
