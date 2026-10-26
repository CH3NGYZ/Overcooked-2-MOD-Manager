<script setup lang="ts">
defineProps<{ title: string }>()
</script>

<template>
  <section class="settings-group">
    <h2 class="group-title">{{ title }}</h2>
    <div class="group-body">
      <slot />
    </div>
  </section>
</template>

<style scoped>
.settings-group { max-width: 980px; margin-bottom: 22px; }
.group-title {
  margin: 0 0 10px;
  color: var(--oc2-accent);
  font-size: 13px;
  font-weight: 700;
  letter-spacing: .04em;
}
.group-body {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
</style>
