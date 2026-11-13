<script setup lang="ts">
import { computed } from 'vue'
import { pickI18n } from '../i18n'

const props = defineProps<{ label: string; noteI18n?: Record<string, string> }>()
const note = computed(() => pickI18n(props.noteI18n, '').trim())
</script>

<template>
  <span class="source-title">
    <b>{{ label }}</b>
    <span v-if="note" class="source-note"> · {{ note }}</span>
  </span>
</template>

<style scoped>
.source-title { min-width: 0; overflow-wrap: anywhere; }
.source-title b { font-size: 14px; color: var(--oc2-text-1); }
.source-note { color: var(--oc2-text-3); font-size: 12px; font-weight: normal; }
</style>
