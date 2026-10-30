import { createRouter, createWebHashHistory } from 'vue-router'

export const routes = [
  { path: '/', name: 'overview', meta: { title: 'nav.overview' }, component: () => import('../views/OverviewView.vue') },
  { path: '/mods-online', name: 'mods-online', meta: { title: 'nav.modInstall' }, component: () => import('../views/ModsOnlineView.vue') },
  { path: '/mods-local', name: 'mods-local', meta: { title: 'nav.modManager' }, component: () => import('../views/ModsLocalView.vue') },
  { path: '/levels', name: 'levels', meta: { title: 'levelSet.title' }, component: () => import('../views/LevelsView.vue') },
  { path: '/rename', name: 'rename', meta: { title: 'rename.colorfulName' }, component: () => import('../views/RenameView.vue') },
  { path: '/tools', name: 'tools', meta: { title: 'nav.tools' }, component: () => import('../views/ToolsView.vue') },
  { path: '/settings', name: 'settings', meta: { title: 'nav.settings' }, component: () => import('../views/SettingsView.vue') },
]

export const router = createRouter({
  history: createWebHashHistory(),
  routes,
})
