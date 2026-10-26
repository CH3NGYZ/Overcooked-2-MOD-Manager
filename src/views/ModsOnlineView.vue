<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NCollapse, NCollapseItem, NEmpty, NFloatButton, NSpin, NTag } from 'naive-ui'
import { ChevronDownOutline, ChevronUpOutline, DownloadOutline, RefreshOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { useDashboard } from '../composables/useDashboardStore'
import { locale, t } from '../i18n'
import type { Repository, RepositorySource } from '../types'

const message = useMessage()
const { load, loading, config, game } = useDashboard()

const refreshing = ref<Record<string, boolean>>({})
const installing = ref<Record<string, boolean>>({})
const expanded = ref<Record<string, boolean>>({})
const openSources = ref<string[]>([])
const openGroups = ref<string[]>([])

onMounted(() => { void load() })

const repositories = computed<Repository[]>(() => config.value?.repositories ?? [])
const sources = computed<RepositorySource[]>(() => config.value?.repository_sources ?? [])
const busyCount = computed(() => Object.values(refreshing.value).filter(Boolean).length)

/** 多语言取值：i18n 表优先当前语言，回退中文，再回退原字段。 */
function pickI18n(map: Record<string, string> | undefined, fallback: string): string {
  if (!map) return fallback
  return map[locale.value] || map['zh-CN'] || Object.values(map)[0] || fallback
}
function repoName(repo: Repository) {
  return pickI18n(repo.name_i18n, repo.name)
}

interface GroupNode { key: string; label: string; repos: Repository[] }
interface SourceNode { source: RepositorySource; label: string; groups: GroupNode[]; count: number }

/** 三级树：仓库源 → 分类（group_i18n）→ 仓库。 */
const tree = computed<SourceNode[]>(() => {
  const bySource = new Map<string, Repository[]>()
  for (const repo of repositories.value) {
    const key = repo.source_id ?? '__none__'
    const list = bySource.get(key)
    if (list) list.push(repo)
    else bySource.set(key, [repo])
  }

  const nodes: SourceNode[] = []
  const pushNode = (source: RepositorySource, repos: Repository[]) => {
    const buckets = new Map<string, Repository[]>()
    for (const repo of repos) {
      const label = pickI18n(repo.group_i18n, repo.group || '')
      const list = buckets.get(label)
      if (list) list.push(repo)
      else buckets.set(label, [repo])
    }
    const groups: GroupNode[] = [...buckets.entries()]
      .sort(([a], [b]) => (a === '' ? 1 : b === '' ? -1 : a.localeCompare(b)))
      .map(([label, list]) => ({ key: `${source.id}::${label || '__ungrouped__'}`, label, repos: list }))
    nodes.push({ source, label: pickI18n(source.nickname_i18n, source.nickname), groups, count: repos.length })
  }

  for (const source of sources.value) {
    const repos = bySource.get(source.id)
    if (!repos?.length) continue
    pushNode(source, repos)
    bySource.delete(source.id)
  }
  // 未归属已声明源的仓库收进虚拟源，避免被静默隐藏。
  const orphan = bySource.get('__none__')
  if (orphan?.length) {
    pushNode({ id: '__none__', nickname: t('未归类仓库'), url: '', builtin: false }, orphan)
  }
  return nodes
})

function groupLabel(node: GroupNode) {
  return node.label || t('未分组')
}

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
    message.success(`${repoName(repo)}: ` + t('获取到 {count} 个 Release', { count: releases.length }))
    await load()
  } catch (reason) { fail(reason) } finally { refreshing.value[repo.id] = false }
}

async function refreshSource(node: SourceNode) {
  for (const group of node.groups) {
    for (const repo of group.repos) {
      if (!refreshing.value[repo.id]) await refreshOne(repo)
    }
  }
}

async function refreshAll() {
  for (const node of tree.value) await refreshSource(node)
}

function gameBlocked() {
  if (game.value?.kitchen_running) {
    message.warning(t('游戏运行中，已禁止安装。请先关闭游戏。'))
    return true
  }
  return false
}

async function install(repo: Repository, tag?: string) {
  if (gameBlocked()) return
  installing.value[repo.id] = true
  try {
    const result = await api.installRepository(repo.id, tag)
    if (result.ok) message.success(result.message || `${repoName(repo)} ✓`)
    else message.error(result.message)
  } catch (reason) { fail(reason) } finally { installing.value[repo.id] = false }
}

function toggleRelease(repo: Repository) {
  expanded.value[repo.id] = !expanded.value[repo.id]
}

function statusTag(repo: Repository) {
  if (repo.download_mode === 'repository_file') return { type: 'default' as const, text: t('仓库文件') }
  if (!repo.enabled) return { type: 'default' as const, text: t('未启用') }
  if (repo.last_error) return { type: 'error' as const, text: t('刷新失败') }
  return null
}
</script>

<template>
  <n-spin :show="loading">
    <div class="toolbar">
      <span class="muted">{{ t('共 {count} 个仓库', { count: repositories.length }) }}</span>
    </div>

    <n-empty v-if="!loading && !repositories.length" :description="t('暂无仓库配置')" class="empty" />

    <n-collapse v-else v-model:expanded-names="openSources" class="source-collapse">
      <n-collapse-item v-for="node in tree" :key="node.source.id" :name="node.source.id">
        <template #header>
          <div class="source-head">
            <b>{{ node.label }}</b>
            <n-tag size="tiny" :bordered="false">{{ node.count }}</n-tag>
            <n-button size="tiny" quaternary
              :loading="node.groups.some((g) => g.repos.some((r) => refreshing[r.id]))"
              :disabled="busyCount > 0" @click.stop="refreshSource(node)">
              <RefreshOutline width="13" height="13" />
            </n-button>
          </div>
        </template>

        <n-collapse v-model:expanded-names="openGroups" class="group-collapse">
          <n-collapse-item v-for="group in node.groups" :key="group.key" :name="group.key">
            <template #header>
              <div class="group-head">
                <span>{{ groupLabel(group) }}</span>
                <n-tag size="tiny" :bordered="false">{{ group.repos.length }}</n-tag>
              </div>
            </template>

            <div v-for="repo in group.repos" :key="repo.id" class="repo">
              <div class="repo-main">
                <div class="repo-info">
                  <div class="repo-title">
                    <b>{{ repoName(repo) }}</b>
                    <n-tag v-if="statusTag(repo)" size="tiny" :type="statusTag(repo)!.type" :bordered="false">
                      {{ statusTag(repo)!.text }}
                    </n-tag>
                  </div>
                  <div class="muted repo-note">{{ repo.note || repo.repository }}</div>
                </div>

                <div class="repo-meta">
                  <template v-if="repo.download_mode === 'release'">
                    <span class="version mono">{{ latest(repo)?.version || latest(repo)?.tag_name || '—' }}</span>
                    <span class="muted">
                      {{ repo.cached_releases.length
                        ? t('{count} 个版本', { count: repo.cached_releases.length })
                        : t('未刷新') }}
                    </span>
                  </template>
                  <span v-else class="muted mono">{{ repo.branch }}/{{ repo.file_path }}</span>
                </div>

                <div class="repo-acts">
                  <n-button size="small" secondary :loading="!!refreshing[repo.id]" @click="refreshOne(repo)">
                    <RefreshOutline width="14" height="14" />{{ t('刷新') }}
                  </n-button>
                  <n-button size="small" type="primary" :loading="!!installing[repo.id]"
                    :disabled="!!game?.kitchen_running || !!refreshing[repo.id]" @click="install(repo)">
                    <DownloadOutline width="14" height="14" />{{ t('安装') }}
                  </n-button>
                  <n-button size="small" quaternary @click="toggleRelease(repo)">
                    <ChevronUpOutline v-if="expanded[repo.id]" width="14" height="14" />
                    <ChevronDownOutline v-else width="14" height="14" />{{ t('版本') }}
                  </n-button>
                </div>
              </div>

              <div v-if="repo.last_error" class="err muted">{{ repo.last_error }}</div>

              <div v-if="expanded[repo.id] && repo.download_mode === 'release'" class="releases">
                <div v-for="release in repo.cached_releases.slice(0, 5)" :key="release.tag_name" class="release-row">
                  <span class="mono">{{ release.version || release.tag_name }}</span>
                  <span class="muted">{{ release.published_at?.slice(0, 10) }}</span>
                  <n-tag v-if="release.prerelease" size="tiny" type="warning" :bordered="false">{{ t('预发布') }}</n-tag>
                  <n-button size="tiny" secondary :disabled="!!game?.kitchen_running || !!installing[repo.id]"
                    @click="install(repo, release.tag_name)">{{ t('安装此版本') }}</n-button>
                </div>
                <div v-if="!repo.cached_releases.length" class="muted">{{ t('尚无缓存的 Release，请先刷新。') }}</div>
              </div>
            </div>
          </n-collapse-item>
        </n-collapse>
      </n-collapse-item>
    </n-collapse>

    <n-float-button position="fixed" bottom="28" right="32" :disabled="busyCount > 0"
      :tooltip="t('全部刷新（逐仓独立，互不阻塞）')" @click="refreshAll">
      <RefreshOutline width="17" height="17" />
    </n-float-button>
  </n-spin>
</template>

<style scoped>
.toolbar {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 12px;
  margin-bottom: 12px;
}
.empty { margin: 60px 0; }
.source-collapse { max-width: 1080px; }
.source-head { display: flex; align-items: center; gap: 9px; min-width: 0; }
.source-head b { font-size: 14px; color: var(--oc2-text-1); }
.group-collapse { margin-top: -4px; }
.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--oc2-accent);
}
.repo {
  padding: 10px 0;
  border-bottom: 1px dashed var(--oc2-row-line);
}
.repo:last-child { border-bottom: 0; }
.repo-main { display: flex; align-items: center; gap: 18px; }
.repo-info { flex: 1; min-width: 0; }
.repo-title { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.repo-title b { font-size: 13.5px; color: var(--oc2-text-1); }
.repo-note { margin-top: 4px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.repo-meta { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; flex: 0 0 auto; }
.version { color: var(--oc2-accent); font-size: 13px; }
.repo-acts { display: flex; gap: 8px; flex: 0 0 auto; }
.err { margin-top: 8px; color: #ee8585; }
.releases { margin-top: 10px; padding-top: 8px; border-top: 1px dashed var(--oc2-row-line); }
.release-row { display: flex; align-items: center; gap: 14px; padding: 5px 0; }
.release-row .mono { min-width: 90px; }
.release-row .n-button { margin-left: auto; }
</style>
