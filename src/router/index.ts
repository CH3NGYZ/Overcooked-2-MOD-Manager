import { createRouter, createWebHashHistory } from 'vue-router'

export const routes = [
  { path: '/', name: 'overview', meta: { title: '总览' }, component: () => import('../views/OverviewView.vue') },
  { path: '/game-path', name: 'game-path', meta: { title: '游戏路径' }, component: () => import('../views/GamePathView.vue') },
  { path: '/framework', name: 'framework', meta: { title: '框架管理' }, component: () => import('../views/FrameworkView.vue') },
  { path: '/mods-online', name: 'mods-online', meta: { title: 'MOD 安装' }, component: () => import('../views/ModsOnlineView.vue') },
  { path: '/mods-local', name: 'mods-local', meta: { title: 'MOD 管理' }, component: () => import('../views/ModsLocalView.vue') },
  { path: '/settings', name: 'settings', meta: { title: '设置' }, component: () => import('../views/SettingsView.vue') },
]

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
})
