<script setup lang="ts">
import { computed } from 'vue'
import { useRoute } from 'vue-router'
import { NScrollbar } from 'naive-ui'
import {
  CloudDownloadOutline, FileTrayFullOutline, GridOutline, BuildOutline, SettingsOutline,
  ColorWandOutline, MapOutline, AlbumsOutline,
} from '@vicons/ionicons5'
import { t } from '../i18n'

const route = useRoute()

/**
 * 主导航：功能入口，按功能域分组。
 *
 * 分组用数据表达（`sections`）而不是在模板里手写两条 `<div class="nav-divider" />`：
 * 分割线数量必须恒等于「组数 - 1」，模板里写死就只能在加组时靠人记得补一条；
 * 组间顺序也由数组顺序一处决定，而不是散在模板行之间。
 *
 * 「框架管理」不再单独占位 —— BepInEx 的管理已收进「设置 → 路径管理 →
 * BepInEx 框架管理」抽屉，和装一个 MOD 的复杂度相当，不值得常驻导航。
 */
const sections = [
  // 总览是入口页，自成一档：它讲的是一屏之内的摘要，不属于任何具体内容域。
  {
    key: 'overview',
    items: [
      { name: 'overview', title: 'nav.overview', icon: GridOutline },
    ],
  },
  // MOD 与地图（DIYLevel 关卡集）是两套数据，各自成组。
  // 组内下载与管理分开两页：下载页要拉 Release（慢、需要网络），管理页只看本地磁盘。
  {
    key: 'mods',
    items: [
      { name: 'mods-online', title: 'nav.modInstall', icon: CloudDownloadOutline },
      { name: 'mods-local', title: 'nav.modManager', icon: FileTrayFullOutline },
    ],
  },
  {
    key: 'maps',
    items: [
      { name: 'maps-online', title: 'levelSet.download', icon: MapOutline },
      { name: 'levels', title: 'levelSet.title', icon: AlbumsOutline },
    ],
  },
]
/**
 * 最后一条分割线之下：与 MOD 管理配套的工具入口。
 *
 * 单独一个数组而不是 sections 里的一组：它不跟其它组并列（固定排在末尾，
 * 且不参与「按功能域分组」的语义），模板里因此只有这一条分割线是显式的。
 *
 * 回收站**不在这里**：它是低频的维护功能，入口放在「工具」页的
 * 「回收站设置」分组里（打开回收站弹窗），不占主导航。
 */
const extraItems = [
  { name: 'rename', title: 'rename.colorfulName', icon: ColorWandOutline },
]
/** 沉底导航：工具与设置固定在侧栏底部。 */
const bottomItems = [
  { name: 'tools', title: 'nav.tools', icon: BuildOutline },
  { name: 'settings', title: 'nav.settings', icon: SettingsOutline },
]

const activeName = computed(() => (route.name as string) ?? 'overview')
</script>

<template>
  <aside class="sidebar">
    <!-- 可滚动导航：用 Naive 的 n-scrollbar（自带悬浮细滚动条，不占布局宽度） -->
    <n-scrollbar class="nav-scroll">
      <nav class="nav">
        <!-- 组间分割线由组序号推出：第一组之前没有线，之后每组之前一条 -->
        <template v-for="(section, index) in sections" :key="section.key">
          <div v-if="index > 0" class="nav-divider" />
          <router-link v-for="item in section.items" :key="item.name" :to="{ name: item.name }" class="nav-item"
            :class="{ active: activeName === item.name }">
            <component :is="item.icon" width="20" height="20" />
            <span>{{ t(item.title) }}</span>
          </router-link>
        </template>

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
</style>
