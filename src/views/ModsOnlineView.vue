<script setup lang="ts">
import { t } from '../i18n'
import { computed, onMounted, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NCard, NEmpty, NFloatButton, NSpin, NTag } from 'naive-ui'
import { ChevronDownOutline, ChevronUpOutline, DownloadOutline, RefreshOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { useDashboard } from '../composables/useDashboardStore'
import type { Repository } from '../types'

const message = useMessage()
const { load, loading, config } = useDashboard()

const refreshing = ref<Record<string, boolean>>({})
const installing = ref<Record<string, boolean>>({})
const expanded = ref<Record<string, boolean>>({})
const releasesCache = ref<Record<string, number>>({})

onMounted(() => { void load() })

const repositories = computed<Repository[]>(() => config.value?.repositories ?? [])
const busyCount = computed(() => Object.values(refreshing.value).filter(Boolean).length)

function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
}

function latest(repo: Repository) {
  return repo.cached_releases[0]
}

async function refreshOne(repo: Repository) {
  refreshing.value[repo.id] = true
  try {
    const releases = await api.refreshRepository(repo.id)
    releasesCache.value[repo.id] = releases.length
    message.success(`${repo.name}：` + t('获取到 {count} 个 Release', { count: releases.length }))
    await load()
  } catch (reason) { fail(reason) } finally { refreshing.value[repo.id] = false }
}

async function refreshAll() {
  for (const repo of repositories.value) {
    if (refreshing.value[repo.id]) continue
    await refreshOne(repo)
  }
}

async function install(repo: Repository, tag?: string) {
  if (gameBlocked()) return
  installing.value[repo.id] = true
  try {
    const result = await api.installRepository(repo.id, tag)
    result.ok ? message.success(result.message || `${repo.name} 安装完成`) : message.error(result.message)
  } catch (reason) { fail(reason) } finally { installing.value[repo.id] = false }
}

const { game } = useDashboard()
function gameBlocked() {
  if (game.value?.kitchen_running) {
    message.warning(t('游戏运行中，已禁止安装。请先关闭游戏。'))
    return true
  }
  return false
}

function toggleExpand(repo: Repository) {
  expanded.value[repo.id] = !expanded.value[repo.id]
}
</script>

<template>
  <n-spin :show="loading">
    <div class="toolbar">
      <span class="muted">{{ t('共 {count} 个仓库', { count: repositories.length }) }}</span>

    </div>

    <n-empty v-if="!loading && !repositories.length" :description="t('暂无仓库配置')" class="empty" />

    <n-card v-for="repo in repositories" :key="repo.id" class="repo" :bordered="true">
      <div class="repo-main">
        <div class="repo-info">
          <div class="repo-title">
            <b>{{ repo.name }}</b>
            <n-tag size="tiny" :bordered="false">{{ repo.provider === 'gitee' ? 'Gitee' : 'GitHub' }}</n-tag>
            <n-tag v-if="repo.download_mode === 'repository_file'" size="tiny" :bordered="false" type="info">
              仓库文件
            </n-tag>
            <n-tag v-if="!repo.enabled" size="tiny" :bordered="false" type="default">{{ t('未启用') }}</n-tag>
            <n-tag v-if="repo.last_error" size="tiny" type="error" :bordered="false">{{ t('刷新失败') }}</n-tag>
          </div>
          <div class="muted repo-note">{{ repo.note || repo.repository }}</div>
        </div>
        <div class="repo-meta">
          <template v-if="repo.download_mode === 'release'">
            <span class="version mono">{{ latest(repo)?.version || latest(repo)?.tag_name || '—' }}</span>
            <span class="muted">{{ repo.cached_releases.length ? `${repo.cached_releases.length} 个版本` : t('未刷新') }}</span>
          </template>
          <span v-else class="muted mono">{{ repo.branch }}/{{ repo.file_path }}</span>
        </div>
        <div class="repo-acts">
          <n-button size="small" secondary :loading="!!refreshing[repo.id]" @click="refreshOne(repo)">
            <RefreshOutline width="14" height="14" />{{ t('刷新') }}</n-button>
          <n-button size="small" type="primary" :loading="!!installing[repo.id]"
            :disabled="game?.kitchen_running || !!refreshing[repo.id]" @click="install(repo)">
            <DownloadOutline width="14" height="14" />{{ t('安装') }}</n-button>
          <n-button size="small" quaternary @click="toggleExpand(repo)">
            <ChevronUpOutline v-if="expanded[repo.id]" width="14" height="14" />
            <ChevronDownOutline v-else width="14" height="14" />{{ t('版本') }}</n-button>
        </div>
      </div>
      <div v-if="repo.last_error" class="err muted">{{ repo.last_error }}</div>
      <div v-if="expanded[repo.id] && repo.download_mode === 'release'" class="releases">
        <div v-for="release in repo.cached_releases.slice(0, 5)" :key="release.tag_name" class="release-row">
          <span class="mono">{{ release.version || release.tag_name }}</span>
          <span class="muted">{{ release.published_at?.slice(0, 10) }}</span>
          <n-tag v-if="release.prerelease" size="tiny" type="warning" :bordered="false">{{ t('预发布') }}</n-tag>
          <n-button size="tiny" secondary
            :disabled="game?.kitchen_running || !!installing[repo.id]"
            @click="install(repo, release.tag_name)">{{ t('安装此版本') }}</n-button>
        </div>
        <div v-if="!repo.cached_releases.length" class="muted">{{ t('尚无缓存的 Release，请先刷新。') }}</div>
      </div>
    </n-card>
  </n-spin>
    <n-float-button position="fixed" bottom="28" right="32" :disabled="busyCount > 0"
      :tooltip="t('全部刷新（逐仓独立，互不阻塞）')" @click="refreshAll">
      <RefreshOutline width="17" height="17" :class="{ spin: busyCount > 0 }" />
    </n-float-button>
</template>

<style scoped>
.toolbar {
  display: flex; align-items: center; justify-content: flex-end; gap: 12px;
  margin-bottom: 14px;
}
.empty { margin: 60px 0; }
.repo { max-width: 980px; margin-bottom: 12px; }
.repo-main {
  display: flex; align-items: center; gap: 18px;
}
.repo-info { flex: 1; min-width: 0; }
.repo-title { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.repo-title b { font-size: 14px; }
.repo-note { margin-top: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.repo-meta { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; flex: 0 0 auto; }
.version { color: var(--oc2-accent); font-size: 13px; }
.repo-acts { display: flex; gap: 8px; flex: 0 0 auto; }
.err { margin-top: 8px; color: #ee8585; }
.releases {
  margin-top: 12px; padding-top: 10px; border-top: 1px dashed var(--oc2-row-line);
}
.release-row {
  display: flex; align-items: center; gap: 14px; padding: 6px 0;
}
.release-row .mono { min-width: 90px; }
.release-row .n-button { margin-left: auto; }
</style>
