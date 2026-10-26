<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { CloudDownloadOutline, CubeOutline, FileTrayFullOutline, GridOutline, BuildOutline, SettingsOutline } from '@vicons/ionicons5'
import { t } from '../i18n'

defineProps<{ version: string }>()
const route = useRoute()

const navItems = [
  { name: 'overview', title: '总览', icon: GridOutline },
  { name: 'framework', title: '框架管理', icon: CubeOutline },
  { name: 'mods-online', title: 'MOD 安装', icon: CloudDownloadOutline },
  { name: 'mods-local', title: 'MOD 管理', icon: FileTrayFullOutline },
  { name: 'tools', title: '工具', icon: BuildOutline },
  { name: 'settings', title: '设置', icon: SettingsOutline },
]

const activeName = computed(() => (route.name as string) ?? 'overview')
</script>

<template>
  <aside class="sidebar">
    <nav class="nav">
      <router-link v-for="item in navItems" :key="item.name" :to="{ name: item.name }" class="nav-item"
        :class="{ active: activeName === item.name }">
        <component :is="item.icon" width="20" height="20" />
        <span>{{ t(item.title) }}</span>
      </router-link>
    </nav>
    <div class="side-foot muted">{{ version ? `v${version}` : '' }}</div>
  </aside>
</template>

<style scoped>
/* Astral 式窄侧栏：图标在上、文字在下的方块导航；配色与组件风格仍走 Naive 主题。 */
.sidebar {
  width: 96px;
  flex: 0 0 96px;
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  border-right: 1px solid var(--oc2-rail-line);
  background: var(--oc2-rail);
  padding: 14px 8px 12px;
}
.nav {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
  flex: 1 1 auto;
  min-height: 0;
  overflow-y: auto;
}
.nav-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 9px 0 7px;
  border-radius: 11px;
  color: var(--oc2-text-2);
  text-decoration: none;
  font-size: 11px;
  transition: background-color .16s ease, color .16s ease;
}
.nav-item:hover {
  background: var(--oc2-row-hover);
  color: var(--oc2-text-1);
}
.nav-item.active {
  background: rgba(60, 184, 155, .14);
  color: var(--oc2-accent);
  font-weight: 600;
}
.side-foot {
  margin-top: auto;
  font-size: 10.5px;
}
</style>
