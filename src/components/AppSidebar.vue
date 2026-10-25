<script setup lang="ts">
import { computed, type Component } from 'vue'
import type { GameInfo } from '../types'
import { t } from '../i18n'
import gameIcon from '../assets/overcooked2icon.jpg'

type View = 'overview' | 'framework' | 'installed' | 'repositories' | 'sponsor'

const props = defineProps<{
  navItems: Array<{ id: View; label: string; hint: string; icon: Component }>
  activeView: View
  game?: GameInfo
  appVersion: string
  appIcon: string
  collapsed: boolean
  launchNormal?: () => unknown
}>()

const emit = defineEmits<{ 'change-view': [view: View]; toggle: [] }>()
const navigationGroups = computed(() => [
  { id: 'workspace', label: '工作区', items: props.navItems.filter((item) => item.id !== 'sponsor') },
  { id: 'bottom', label: '赞助渠道', items: props.navItems.filter((item) => item.id === 'sponsor') },
])
const kitchenRunning = computed(() => !!props.game?.kitchen_running)

function launchKitchen() {
  // 单向入口：运行中点击不重复拉起，也不关闭游戏。
  if (kitchenRunning.value) return
  void props.launchNormal?.()
}
</script>

<template>
  <aside class="sidebar" :class="{ 'sidebar-collapsed': collapsed }">
    <button type="button" class="brand" :title="t(collapsed ? '展开侧边栏' : '收起侧边栏')"
      :aria-label="t(collapsed ? '展开侧边栏' : '收起侧边栏')" :aria-expanded="!collapsed" @click="emit('toggle')">
      <span class="brand-mark"><img :src="appIcon" alt="" /></span>
      <span class="brand-text"><strong>{{ t('OC2 模组管理器') }}</strong></span>
    </button>
    <div class="sidebar-section-label">{{ t('工作区') }}</div>
    <nav v-for="group in navigationGroups" :key="group.id" class="nav-list" :class="`nav-list-${group.id}`"
      :aria-label="t(group.label)"><button v-for="item in group.items" :key="item.id" class="nav-item"
        :data-tutorial="`nav-${item.id}`" :class="{ active: activeView === item.id }"
        :title="collapsed ? t(item.label) : undefined" @click="emit('change-view', item.id)">
        <component :is="item.icon" :size="22" stroke-width="1.8" /><span class="nav-label"><b>{{ t(item.label)
          }}</b><small>{{ t(item.hint) }}</small></span>
      </button><button v-if="group.id === 'bottom'" type="button" class="nav-item game-launch" data-tutorial="game-launch"
        :aria-label="t('通过 Steam 启动胡闹厨房2（运行中点击无操作）')"
        :title="t('通过 Steam 启动胡闹厨房2（运行中点击无操作）')" @click="launchKitchen">
        <img class="game-icon" :class="{ off: !kitchenRunning }" :src="gameIcon" alt="" /><span class="nav-label"><b>{{
          kitchenRunning ? t('游戏运行中') : t('启动游戏') }}</b></span>
      </button></nav>
    <div class="sidebar-foot">
      <span class="version-tag">{{ appVersion ? 'v' + appVersion : '' }}</span>
    </div>
  </aside>
</template>
