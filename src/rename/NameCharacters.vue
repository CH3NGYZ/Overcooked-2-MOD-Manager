<script setup lang="ts">
import type { StyledChar } from '../colorfulName'

defineProps<{ chars: readonly StyledChar[]; selection?: { start: number; end: number } | null }>()
</script>

<template>
  <span v-for="({ char, color, size }, index) in chars" :key="index" :data-index="index" class="name-character"
    :class="{ 'is-selected': selection && index >= selection.start && index < selection.end }"
    :style="{ color, fontSize: `${size}px` }">{{ char }}</span>
</template>

<style>
.name-preview-surface {
  background: #f2f4f7;
  white-space: pre-wrap;
  overflow-wrap: anywhere;
}
</style>

<style scoped>
.name-character.is-selected {
  background: color-mix(in srgb, var(--oc2-accent) 22%, transparent);
  box-shadow: 0 0 0 1px color-mix(in srgb, var(--oc2-accent) 55%, transparent);
  border-radius: 3px;
}
</style>
