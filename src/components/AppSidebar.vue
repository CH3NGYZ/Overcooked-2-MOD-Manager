<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { Boxes, CloudDownload, Gamepad2, LayoutGrid, PackageCheck, Settings } from 'lucide-vue-next'

const props = defineProps<{ version: string }>()
const route = useRoute()

const navItems = [
  { name: 'overview', title: '总览', icon: LayoutGrid },
  { name: 'game-path', title: '游戏路径', icon: Gamepad2 },
  { name: 'framework', title: '框架管理', icon: PackageCheck },
  { name: 'mods-online', title: 'MOD 安装', icon: CloudDownload },
  { name: 'mods-local', title: 'MOD 管理', icon: Boxes },
  { name: 'settings', title: '设置', icon: Settings },
]

const activeName = computed(() => (route.name as string) ?? 'overview')
</script>

<template>
  <aside class="sidebar">
    <div class="brand">
      <div class="brand-mark"><Boxes :size="22" :stroke-width="2" /></div>
      <div class="brand-text">
        <strong>OC2 MOD 管理器</strong>
        <small>游戏模组管理工具</small>
      </div>
    </div>
    <nav class="nav">
      <router-link v-for="item in navItems" :key="item.name" :to="{ name: item.name }" class="nav-item"
        :class="{ active: activeName === item.name }">
        <component :is="item.icon" :size="19" :stroke-width="1.9" />
        <span>{{ item.title }}</span>
      </router-link>
    </nav>
    <div class="side-foot">
      <span class="muted">{{ version ? `v${version}` : '' }}</span>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: 232px;
  flex: 0 0 232px;
  display: flex;
  flex-direction: column;
  border-right: 1px solid #232a33;
  background: #17191d;
  padding: 18px 12px 14px;
}
.brand {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 2px 10px 20px;
}
.brand-mark {
  width: 40px;
  height: 40px;
  border-radius: 10px;
  background: #5b6ee8;
  color: #fff;
  display: grid;
  place-items: center;
}
.brand-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.brand-text strong {
  font-size: 15px;
  font-weight: 700;
}
.brand-text small {
  color: #8d9bab;
  font-size: 11px;
  margin-top: 2px;
}
.nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 12px;
  height: 42px;
  padding: 0 14px;
  border-radius: 9px;
  color: #aeb9c6;
  text-decoration: none;
  font-size: 13.5px;
  transition: background-color .16s ease, color .16s ease;
}
.nav-item:hover {
  background: #1f242b;
  color: #e8edf3;
}
.nav-item.active {
  background: #23303a;
  color: #3cb89b;
  font-weight: 600;
}
.side-foot {
  margin-top: auto;
  padding: 10px 12px 0;
  border-top: 1px solid #232a33;
}
</style>
