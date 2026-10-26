<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NCollapse, NCollapseItem, NDrawer, NDrawerContent, NEmpty, NFloatButton, NInput, NSpin, NTag } from 'naive-ui'
import { DownloadOutline, RefreshOutline, SearchOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { useDashboard } from '../composables/useDashboardStore'
import { locale, t } from '../i18n'
import { repositoryMatchesRegion, repositoryRegionLabel, sourceMatchesRegion } from '../repositorySources'
import RepositoryDetail from '../components/RepositoryDetail.vue'
import type { Repository, RepositorySource } from '../types'

const message = useMessage()
const { load, loading, config, game } = useDashboard()

const refreshing = ref<Record<string, boolean>>({})
const installing = ref<Record<string, boolean>>({})
const search = ref('')
/** 当前展开详情的仓库（同页展开，不做二级路由）。 */
const detailId = ref<string | null>(null)

const COMMUNITY_IDS = new Set(['github-community', 'gitee-community'])

onMounted(() => { void load() })

const currentRegion = computed(() => config.value?.region)
const regionLabel = computed(() => repositoryRegionLabel(currentRegion.value))

/** 仅保留当前下载渠道适用的仓库。 */
const repositories = computed<Repository[]>(() =>
  (config.value?.repositories ?? []).filter((repo) => repositoryMatchesRegion(repo, currentRegion.value)),
)
const sources = computed<RepositorySource[]>(() =>
  (config.value?.repository_sources ?? []).filter((source) => sourceMatchesRegion(source, currentRegion.value)),
)
const busyCount = computed(() => Object.values(refreshing.value).filter(Boolean).length)

const detailRepo = computed(() => repositories.value.find((repo) => repo.id === detailId.value) ?? null)


/** 多语言取值：优先当前语言 → 中文 → 首个 → 原字段。 */
function pickI18n(map: Record<string, string> | undefined, fallback: string): string {
  if (!map) return fallback
  return map[locale.value] || map['zh-CN'] || Object.values(map)[0] || fallback
}
function repoName(repo: Repository) {
  return pickI18n(repo.name_i18n, repo.name)
}
function repoGroup(repo: Repository) {
  return pickI18n(repo.group_i18n, repo.group || '') || t('未分组')
}
/**
 * 卡片简介：note_i18n 取当前语言，去掉 Markdown 标记只留纯文本首行
 * （卡片是摘要，Markdown 渲染在详情抽屉里做）。
 */
function repoNote(repo: Repository) {
  const raw = pickI18n(repo.note_i18n, repo.note || '')
  const plain = raw
    .replace(/```[\s\S]*?```/g, ' ')
    .replace(/!\[[^\]]*\]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
    .replace(/^#{1,6}\s+/gm, '')
    .replace(/[*_`>]/g, '')
    .replace(/\s+/g, ' ')
    .trim()
  return plain
}

/* ===== 搜索 ===== */
const query = computed(() => search.value.trim().toLowerCase())
function matches(repo: Repository) {
  if (!query.value) return true
  const haystack = [
    repoName(repo),
    repo.name,
    repo.note,
    ...Object.values(repo.note_i18n ?? {}),
    repoGroup(repo),
    repo.group ?? '',
    repo.username,
    repo.repository,
    repo.provider,
  ].join(' ').toLowerCase()
  return haystack.includes(query.value)
}

const visibleCount = computed(() => repositories.value.filter(matches).length)

/* ===== 分组树 ===== */
interface GroupNode { key: string; label: string; repos: Repository[] }
interface SourceNode { source: RepositorySource; label: string; groups: GroupNode[]; count: number; community: boolean }

const openSources = ref<string[]>([])
const openGroups = ref<string[]>([])

const tree = computed<SourceNode[]>(() => {
  const bySource = new Map<string, Repository[]>()
  for (const repo of repositories.value) {
    if (!matches(repo)) continue
    const key = repo.source_id ?? '__none__'
    const list = bySource.get(key)
    if (list) list.push(repo)
    else bySource.set(key, [repo])
  }

  const nodes: SourceNode[] = []
  const push = (source: RepositorySource, repos: Repository[]) => {
    const buckets = new Map<string, Repository[]>()
    for (const repo of repos) {
      const label = repoGroup(repo)
      const list = buckets.get(label)
      if (list) list.push(repo)
      else buckets.set(label, [repo])
    }
    const groups: GroupNode[] = [...buckets.entries()]
      .sort(([a], [b]) => (a === t('未分组') ? 1 : b === t('未分组') ? -1 : a.localeCompare(b)))
      .map(([label, list]) => ({ key: `${source.id}::${label}`, label, repos: list }))
    nodes.push({
      source,
      label: pickI18n(source.nickname_i18n, source.nickname),
      groups,
      count: repos.length,
      community: COMMUNITY_IDS.has(source.id),
    })
  }

  // 按 config.repository_sources 的持久化顺序输出（读取顺序 = 展示顺序）
  for (const source of sources.value) {
    const repos = bySource.get(source.id)
    if (!repos?.length) continue
    push(source, repos)
    bySource.delete(source.id)
  }
  const orphan = bySource.get('__none__')
  if (orphan?.length) push({ id: '__none__', nickname: t('未归类仓库'), url: '', builtin: false }, orphan)

  // 社区源固定放最下方（内部仍保持原相对顺序）
  const normal = nodes.filter((node) => !node.community)
  const community = nodes.filter((node) => node.community)
  return [...normal, ...community]
})

const hasCommunity = computed(() => tree.value.some((node) => node.community))
const normalTree = computed(() => tree.value.filter((node) => !node.community))
const communityTree = computed(() => tree.value.filter((node) => node.community))

// 默认全部展开（源与分类），搜索时也保持展开以便直接看到结果。
const allSourceKeys = computed(() => tree.value.map((node) => node.source.id))
const allGroupKeys = computed(() => tree.value.flatMap((node) => node.groups.map((group) => group.key)))
const sourcesExpanded = computed(() => (query.value ? allSourceKeys.value : (openSources.value.length ? openSources.value : allSourceKeys.value)))
const groupsExpanded = computed(() => (query.value ? allGroupKeys.value : (openGroups.value.length ? openGroups.value : allGroupKeys.value)))

/* ===== 动作 ===== */
function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
}

function latest(repo: Repository) {
  return repo.cached_releases[0]
}

/** 下载量：优先最新 Release 首个匹配资产的 download_count。 */
function downloadCount(repo: Repository) {
  const release = latest(repo)
  if (!release) return null
  const total = release.assets.reduce((sum, asset) => sum + (asset.download_count ?? 0), 0)
  return total || null
}

function statusTag(repo: Repository) {
  if (repo.download_mode === 'repository_file') return { type: 'default' as const, text: t('仓库文件') }
  if (!repo.enabled) return { type: 'default' as const, text: t('未启用') }
  if (repo.last_error) return { type: 'error' as const, text: t('刷新失败') }
  if (latest(repo)) return { type: 'success' as const, text: t('已就绪') }
  return { type: 'warning' as const, text: t('未刷新') }
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

/** 顶栏「刷新所有仓库」：按展示顺序逐仓刷新（互不阻塞 UI）。 */
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

function tileColor(repo: Repository) {
  let hash = 0
  for (const ch of repo.id) hash = (hash * 31 + ch.charCodeAt(0)) % 360
  return { background: `hsl(${hash} 32% 28%)` }
}

defineExpose({ refreshAll, visibleCount })
</script>

<template>
    <!-- 页头 h1 右侧：当前下载渠道 -->
    <Teleport to="#page-header-title-extra" defer>
      <n-tag size="small" :bordered="false" type="info">{{ regionLabel }}</n-tag>
    </Teleport>

    <!-- 页头右侧：计数 + 搜索 + 刷新全部 -->
    <Teleport to="#page-header-actions" defer>
      <span class="header-count muted">{{ t('共 {count} 个仓库', { count: visibleCount }) }}</span>
      <n-input v-model:value="search" clearable size="small" class="header-search"
        :placeholder="t('搜索作者 / MOD 名称 / 简介 / 分组')">
        <template #prefix><SearchOutline width="14" height="14" /></template>
      </n-input>
      <n-button size="small" secondary :loading="busyCount > 0" :disabled="busyCount > 0" @click="refreshAll">
        <RefreshOutline width="14" height="14" />{{ t('刷新全部仓库源') }}
      </n-button>
    </Teleport>

  <n-spin :show="loading">
    <n-empty v-if="!loading && !visibleCount" :description="t('没有匹配的仓库')" class="empty" />

    <template v-else>
      <n-collapse :expanded-names="sourcesExpanded" class="source-collapse"
        @update:expanded-names="(names: string[]) => (openSources = names)">
        <n-collapse-item v-for="node in normalTree" :key="node.source.id" :name="node.source.id">
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

          <n-collapse :expanded-names="groupsExpanded" class="group-collapse"
            @update:expanded-names="(names: string[]) => (openGroups = names)">
            <n-collapse-item v-for="group in node.groups" :key="group.key" :name="group.key">
              <template #header>
                <div class="group-head">
                  <span>{{ group.label }}</span>
                  <n-tag size="tiny" :bordered="false">{{ group.repos.length }}</n-tag>
                </div>
              </template>

              <div class="repo-grid">
                <article v-for="repo in group.repos" :key="repo.id" class="repo-card"
                  :class="{ active: detailId === repo.id }" @click="detailId = repo.id">
                  <div class="card-head">
                    <span class="tile" :style="tileColor(repo)">{{ (repoName(repo)[0] || '?').toUpperCase() }}</span>
                    <span class="card-title">{{ repoName(repo) }}</span>
                    <n-button size="tiny" type="primary" class="card-install" :loading="!!installing[repo.id]"
                      :disabled="!!game?.kitchen_running || !!refreshing[repo.id]" @click.stop="install(repo)">
                      <DownloadOutline width="13" height="13" />{{ t('安装') }}
                    </n-button>
                  </div>

                  <p class="card-desc">{{ repoNote(repo) || repo.repository }}</p>

                  <div class="card-foot">
                    <span class="stat">
                      <span class="muted">{{ t('最新版本') }}</span>
                      <b class="mono">{{ latest(repo)?.version || latest(repo)?.tag_name || '—' }}</b>
                    </span>
                    <span class="stat">
                      <span class="muted">{{ t('下载量') }}</span>
                      <b>{{ downloadCount(repo)?.toLocaleString() ?? '—' }}</b>
                    </span>
                    <n-tag size="tiny" :type="statusTag(repo).type" :bordered="false">{{ statusTag(repo).text }}</n-tag>
                    <span class="card-acts">
                      <n-button size="tiny" quaternary :loading="!!refreshing[repo.id]" @click.stop="refreshOne(repo)">
                        <RefreshOutline width="13" height="13" />
                      </n-button>
                      <span class="detail-hint">{{ t('详情') }} ›</span>
                    </span>
                  </div>

                  <div v-if="repo.last_error" class="err muted">{{ repo.last_error }}</div>
                </article>
              </div>
            </n-collapse-item>
          </n-collapse>
        </n-collapse-item>
      </n-collapse>

      <!-- 社区仓库：与上方仓库源用分割线隔开，固定在最下方 -->
      <template v-if="hasCommunity">
        <div class="community-divider">
          <span>{{ t('社区仓库') }}</span>
        </div>
        <n-collapse :expanded-names="sourcesExpanded" class="source-collapse"
          @update:expanded-names="(names: string[]) => (openSources = names)">
          <n-collapse-item v-for="node in communityTree" :key="node.source.id" :name="node.source.id">
            <template #header>
              <div class="source-head">
                <b>{{ node.label }}</b>
                <n-tag size="tiny" type="warning" :bordered="false">{{ t('来源未知') }}</n-tag>
                <n-tag size="tiny" :bordered="false">{{ node.count }}</n-tag>
              </div>
            </template>
            <n-collapse :expanded-names="groupsExpanded" class="group-collapse"
              @update:expanded-names="(names: string[]) => (openGroups = names)">
              <n-collapse-item v-for="group in node.groups" :key="group.key" :name="group.key">
                <template #header>
                  <div class="group-head">
                    <span>{{ group.label }}</span>
                    <n-tag size="tiny" :bordered="false">{{ group.repos.length }}</n-tag>
                  </div>
                </template>
                <div class="repo-grid">
                  <article v-for="repo in group.repos" :key="repo.id" class="repo-card"
                    :class="{ active: detailId === repo.id }" @click="detailId = repo.id">
                    <div class="card-head">
                      <span class="tile" :style="tileColor(repo)">{{ (repoName(repo)[0] || '?').toUpperCase() }}</span>
                      <span class="card-title">{{ repoName(repo) }}</span>
                      <n-button size="tiny" type="primary" class="card-install" :loading="!!installing[repo.id]"
                        :disabled="!!game?.kitchen_running || !!refreshing[repo.id]" @click.stop="install(repo)">
                        <DownloadOutline width="13" height="13" />{{ t('安装') }}
                      </n-button>
                    </div>
                    <p class="card-desc">{{ repoNote(repo) || repo.repository }}</p>
                    <div class="card-foot">
                      <span class="stat">
                        <span class="muted">{{ t('最新版本') }}</span>
                        <b class="mono">{{ latest(repo)?.version || latest(repo)?.tag_name || '—' }}</b>
                      </span>
                      <span class="stat">
                        <span class="muted">{{ t('下载量') }}</span>
                        <b>{{ downloadCount(repo)?.toLocaleString() ?? '—' }}</b>
                      </span>
                      <n-tag size="tiny" :type="statusTag(repo).type" :bordered="false">{{ statusTag(repo).text }}</n-tag>
                      <span class="card-acts">
                        <n-button size="tiny" quaternary :loading="!!refreshing[repo.id]" @click.stop="refreshOne(repo)">
                          <RefreshOutline width="13" height="13" />
                        </n-button>
                        <span class="detail-hint">{{ t('详情') }} ›</span>
                      </span>
                    </div>
                    <div v-if="repo.last_error" class="err muted">{{ repo.last_error }}</div>
                  </article>
                </div>
              </n-collapse-item>
            </n-collapse>
          </n-collapse-item>
        </n-collapse>
      </template>
    </template>

    <n-float-button position="fixed" bottom="28" right="32" :disabled="busyCount > 0"
      :tooltip="t('全部刷新（逐仓独立，互不阻塞）')" @click="refreshAll">
      <RefreshOutline width="17" height="17" />
    </n-float-button>
  </n-spin>

  <!-- 点击卡片：从右侧滑出仓库详情（指标 / 链接 / 备注 / README） -->
  <n-drawer :show="!!detailRepo" :width="720" placement="right" @update:show="(value: boolean) => { if (!value) detailId = null }">
    <n-drawer-content v-if="detailRepo" :native-scrollbar="false" closable>
      <RepositoryDetail :repo="detailRepo" :installing="!!installing[detailRepo.id]" :game-blocked="!!game?.kitchen_running"
        @close="detailId = null" @install="install(detailRepo!)" />
    </n-drawer-content>
  </n-drawer>
</template>

<style scoped>
.empty { margin: 60px 0; }
.source-collapse { width: 100%; }

/* 三级固定缩进：源 0、分类 14px、卡片 28px */
.source-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
.source-head b { font-size: 14px; color: var(--oc2-text-1); }
.group-collapse { margin: -4px 0 0 14px; }
.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--oc2-accent);
}
.repo-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(310px, 1fr));
  gap: 12px;
  margin: 2px 0 6px 28px;
}

/* 卡片：圆角边缘 + 内部分区（标题行 / 描述 / 指标行） */
.repo-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
  transition: border-color .16s ease, background-color .16s ease, box-shadow .16s ease;
}
.repo-card:hover {
  border-color: var(--oc2-accent);
  background: var(--oc2-row-hover);
  box-shadow: 0 4px 16px rgba(0, 0, 0, .16);
}
.repo-card { cursor: pointer; }
.repo-card.active {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 8%, var(--oc2-card));
}
.detail-hint {
  font-size: 11.5px;
  color: var(--oc2-text-4);
  white-space: nowrap;
  transition: color .16s ease;
}
.repo-card:hover .detail-hint { color: var(--oc2-accent); }
.card-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
.tile {
  width: 30px;
  height: 30px;
  flex: 0 0 30px;
  border-radius: 8px;
  display: grid;
  place-items: center;
  font-size: 13px;
  font-weight: 700;
  color: #fff;
}
.card-title {
  flex: 1;
  min-width: 0;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.card-install { flex: 0 0 auto; }
.card-desc {
  margin: 0;
  font-size: 12px;
  line-height: 1.55;
  color: var(--oc2-text-3);
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
  min-height: 37px;
}
.card-foot {
  display: flex;
  align-items: center;
  gap: 14px;
  padding-top: 8px;
  border-top: 1px solid var(--oc2-row-line);
  flex-wrap: wrap;
}
.stat { display: flex; flex-direction: column; gap: 1px; min-width: 0; }
.stat b { font-size: 12.5px; color: var(--oc2-text-1); }
.card-acts { display: flex; gap: 2px; margin-left: auto; }
.err { color: #ee8585; }
.releases {
  padding-top: 8px;
  border-top: 1px dashed var(--oc2-row-line);
  display: grid;
  gap: 6px;
}
.release-row { display: flex; align-items: center; gap: 10px; }
.release-row .n-button { margin-left: auto; }

.community-divider {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 20px 0 12px;
  width: 100%;
  color: var(--oc2-text-3);
  font-size: 12px;
}
.community-divider::before,
.community-divider::after {
  content: '';
  flex: 1;
  height: 1px;
  background: var(--oc2-row-line);
}
.header-count { white-space: nowrap; }
.header-search { width: 240px; }
</style>
