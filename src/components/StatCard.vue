<script setup lang="ts">
import { NCard } from 'naive-ui'

defineProps<{
  label: string
  value: string
  sub?: string
  tone?: 'default' | 'success' | 'warning' | 'danger'
}>()
</script>

<template>
  <n-card class="stat-card" :bordered="true">
    <div class="stat-label">{{ label }}</div>
    <div class="stat-value" :class="tone ? tone : 'default'">{{ value }}</div>
    <div v-if="sub" class="muted">{{ sub }}</div>
  </n-card>
</template>

<style scoped>
.stat-card {
  min-height: 128px;
}
.stat-label {
  color: var(--oc2-text-3);
  font-size: 12.5px;
}
.stat-value {
  margin: 10px 0 6px;
  font-size: 30px;
  font-weight: 700;
  line-height: 1.1;
}
.stat-value.success { color: var(--oc2-accent); }
.stat-value.warning { color: var(--oc2-accent); }
.stat-value.danger { color: #ee8585; }
</style>
