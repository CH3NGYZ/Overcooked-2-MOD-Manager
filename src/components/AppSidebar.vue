<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { CloudDownloadOutline, CubeOutline, FileTrayFullOutline, GridOutline, BuildOutline, SettingsOutline } from '@vicons/ionicons5'

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
    <div class="brand" title="OC2 MOD 管理器">
      <div class="brand-mark"><FileTrayFullOutline width="22" height="22" /></div>
    </div>
    <nav class="nav">
      <router-link v-for="item in navItems" :key="item.name" :to="{ name: item.name }" class="nav-item"
        :class="{ active: activeName === item.name }">
        <component :is="item.icon" width="20" height="20" />
        <span>{{ item.title }}</span>
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
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 8px;
  border-right: 1px solid #232a33;
  background: #16181c;
  padding: 14px 8px 12px;
}
.brand {
  margin-bottom: 6px;
}
.brand-mark {
  width: 42px;
  height: 42px;
  border-radius: 12px;
  background: #5b6ee8;
  color: #fff;
  display: grid;
  place-items: center;
}
.nav {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
}
.nav-item {
  display: flex;
  flex-direction: column;
  align-items: center;
  gap: 4px;
  padding: 9px 0 7px;
  border-radius: 11px;
  color: #aeb9c6;
  text-decoration: none;
  font-size: 11px;
  transition: background-color .16s ease, color .16s ease;
}
.nav-item:hover {
  background: #1f242b;
  color: #e8edf3;
}
.nav-item.active {
  background: rgba(60, 184, 155, .14);
  color: #3cb89b;
  font-weight: 600;
}
.side-foot {
  margin-top: auto;
  font-size: 10.5px;
}
</style>
