<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { NScrollbar } from 'naive-ui'
import {
  CloudDownloadOutline, CubeOutline, FileTrayFullOutline, GridOutline, BuildOutline, SettingsOutline,
  ColorWandOutline, TrashOutline,
} from '@vicons/ionicons5'
import { t } from '../i18n'

defineProps<{ version: string }>()
const route = useRoute()

/** 主导航：功能入口。 */
const mainItems = [
  { name: 'overview', title: '总览', icon: GridOutline },
  { name: 'framework', title: '框架管理', icon: CubeOutline },
  { name: 'mods-online', title: 'MOD 安装', icon: CloudDownloadOutline },
  { name: 'mods-local', title: 'MOD 管理', icon: FileTrayFullOutline },
]
/**
 * 分割线之下：与 MOD 管理配套的工具入口。
 *
 * 回收站是 MOD 管理的下游页面（删除的 MOD 暂存在这里、可恢复或彻底删除），
 * 所以紧挨「MOD 管理」放在同一组，而不是沉底到工具区。
 */
const extraItems = [
  { name: 'mods-recycle', title: '回收站', icon: TrashOutline },
  { name: 'rename', title: '改多彩名', icon: ColorWandOutline },
]
/** 沉底导航：工具与设置固定在侧栏底部。 */
const bottomItems = [
  { name: 'tools', title: '工具', icon: BuildOutline },
  { name: 'settings', title: '设置', icon: SettingsOutline },
]

const activeName = computed(() => (route.name as string) ?? 'overview')
</script>

<template>
  <aside class="sidebar">
    <!-- 可滚动导航：用 Naive 的 n-scrollbar（自带悬浮细滚动条，不占布局宽度） -->
    <n-scrollbar class="nav-scroll">
      <nav class="nav">
        <router-link v-for="item in mainItems" :key="item.name" :to="{ name: item.name }" class="nav-item"
          :class="{ active: activeName === item.name }">
          <component :is="item.icon" width="20" height="20" />
          <span>{{ t(item.title) }}</span>
        </router-link>

        <div class="nav-divider" />

        <router-link v-for="item in extraItems" :key="item.name" :to="{ name: item.name }" class="nav-item"
          :class="{ active: activeName === item.name }">
          <component :is="item.icon" width="20" height="20" />
          <span>{{ t(item.title) }}</span>
        </router-link>
      </nav>
    </n-scrollbar>

    <!-- 固定区：横条紧贴 nav 底部，不留空白；工具与设置不随上方滚动 -->
    <nav class="nav nav-bottom">
      <router-link v-for="item in bottomItems" :key="item.name" :to="{ name: item.name }" class="nav-item"
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
  width: 108px;
  flex: 0 0 108px;
  height: 100%;
  min-height: 0;
  display: flex;
  flex-direction: column;
  align-items: center;
  /* 滚动区与固定区之间不留 gap：横条要直接贴住内容，中间不能露空白 */
  gap: 0;
  border-right: 1px solid var(--oc2-rail-line);
  background: var(--oc2-rail);
  padding: 14px 0 12px;
}
.nav-scroll {
  /* 吃掉剩余高度，把固定区推到底部；滚动条由 n-scrollbar 悬浮绘制，
     不占布局宽度，因此不会挤压导航图标。 */
  flex: 1 1 auto;
  min-height: 0;
  width: 100%;
}
.nav {
  display: flex;
  flex-direction: column;
  gap: 6px;
  width: 100%;
  padding: 0 8px 8px;
}
/*
 * 沉底固定区：紧贴滚动区下沿，顶部一条主题色横条把它框起来。
 * 用 border-top 而不是伪元素 + padding —— 伪元素版本会在横条上方
 * 留出一段会被滚动内容透出来的空白。
 */
.nav-bottom {
  flex: 0 0 auto;
  width: 100%;
  padding: 10px 8px 0;
  /* 不留上外边距：横条直接贴在滚动区下沿，横条上方不能露空白 */
  border-top: 2px solid color-mix(in srgb, var(--oc2-accent) 45%, transparent);
}
.nav-divider {
  /* 不参与 flex 收缩，窗口矮时不会被压成 0 高度 */
  flex: 0 0 auto;
  height: 1px;
  margin: 4px 8px;
  background: var(--oc2-rail-line);
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
  /* 侧栏变矮时由外层滚动，而不是把每一项压扁 */
  flex: 0 0 auto;
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
  margin-top: 2px;
  font-size: 10.5px;
}
</style>
