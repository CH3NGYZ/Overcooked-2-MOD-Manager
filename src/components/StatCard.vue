<script setup lang="ts">
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
  color: #8d9bab;
  font-size: 12.5px;
}
.stat-value {
  margin: 10px 0 6px;
  font-size: 30px;
  font-weight: 700;
  line-height: 1.1;
}
.stat-value.success { color: #3cb89b; }
.stat-value.warning { color: #e6b86a; }
.stat-value.danger { color: #ee8585; }
</style>
