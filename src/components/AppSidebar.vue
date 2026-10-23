<script setup lang="ts">
import { computed, type Component } from 'vue'
import { ChevronLeft, ChevronRight } from 'lucide-vue-next'
import type { GameInfo } from '../types'
import { t } from '../i18n'

type View = 'overview' | 'framework' | 'installed' | 'repositories' | 'sponsor'

const props = defineProps<{
  navItems: Array<{ id: View; label: string; hint: string; icon: Component }>
  activeView: View
  game?: GameInfo
  appVersion: string
  appIcon: string
  collapsed: boolean
}>()

const emit = defineEmits<{ 'change-view': [view: View]; toggle: [] }>()
const navigationGroups = computed(() => [
  { id: 'workspace', label: '工作区', items: props.navItems.filter((item) => item.id !== 'sponsor') },
  { id: 'bottom', label: '赞助渠道', items: props.navItems.filter((item) => item.id === 'sponsor') },
])
</script>

<template>
  <aside class="sidebar" :class="{ 'sidebar-collapsed': collapsed }">
    <div class="brand">
      <div class="brand-mark"><img :src="appIcon" alt="" /></div>
      <div class="brand-text"><strong>{{ t('OC2 模组管理器') }}</strong></div>
      <button type="button" class="sidebar-toggle" :title="t(collapsed ? '展开侧边栏' : '收起侧边栏')"
        :aria-label="t(collapsed ? '展开侧边栏' : '收起侧边栏')" :aria-expanded="!collapsed" @click="emit('toggle')">
        <ChevronRight v-if="collapsed" :size="15" />
        <ChevronLeft v-else :size="15" />
      </button>
    </div>
    <div class="sidebar-section-label">{{ t('工作区') }}</div>
    <nav v-for="group in navigationGroups" :key="group.id" class="nav-list" :class="`nav-list-${group.id}`" :aria-label="t(group.label)"><button v-for="item in group.items" :key="item.id" class="nav-item" :data-tutorial="`nav-${item.id}`"
        :class="{ active: activeView === item.id }" :title="collapsed ? t(item.label) : undefined"
        @click="emit('change-view', item.id)">
        <component :is="item.icon" :size="18" stroke-width="1.8" /><span><b>{{ t(item.label) }}</b><small>{{
          t(item.hint) }}</small></span>
        <ChevronRight :size="14" class="nav-chevron" />
      </button></nav>
    <div class="sidebar-foot">
      <div class="side-health"><span class="pulse"
          :class="game?.kitchen_running ? 'danger-bg' : 'success-bg'"></span><span class="side-health-text">{{ game?.kitchen_running ?
            t('《胡闹厨房2》正在运行') : t('《胡闹厨房2》未在运行') }}</span></div><span class="version-tag">{{ appVersion ? 'v' + appVersion : '' }}</span>
    </div>
  </aside>
</template>
