<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { NButton, NDivider, NSelect, NSkeleton, NTabPane, NTabs, NTag } from 'naive-ui'
import { DownloadOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { t, locale } from '../i18n'
import { renderMermaidBlocks, renderReadme } from '../markdown'
import type { Repository, RepositoryReadme } from '../types'

const props = defineProps<{
  repo: Repository | null
  installing?: boolean
  gameBlocked?: boolean
}>()
/** install 携带选中的 tag（未选则用最新）。 */
const emit = defineEmits<{ close: []; install: [tag?: string]; back: [] }>()

const readme = ref<RepositoryReadme | null>(null)
const loading = ref(false)
const error = ref('')
/** 详情分段：默认显示说明（备注 + README），可滑到「下载安装」。 */
const activeTab = ref<'info' | 'install'>('info')
let requestId: string | null = null

function pickI18n(map: Record<string, string> | undefined, fallback: string) {
  if (!map) return fallback
  return map[locale.value] || map['zh-CN'] || Object.values(map)[0] || fallback
}

const name = computed(() => (props.repo ? pickI18n(props.repo.name_i18n, props.repo.name) : ''))
const note = computed(() => (props.repo ? pickI18n(props.repo.note_i18n, props.repo.note || '') : ''))
/* ===== 版本选择 + 资产规则命中 ===== */
/** 当前选中的 Release（默认最新一个）。 */
const selectedTag = ref<string | null>(null)
const selectedRelease = computed(() => {
  const list = props.repo?.cached_releases ?? []
  const found = list.find((entry) => entry.tag_name === selectedTag.value)
  return found ?? list[0] ?? null
})
const releaseOptions = computed(() =>
  (props.repo?.cached_releases ?? []).map((entry) => ({
    label: entry.version || entry.tag_name,
    // Naive UI 的 n-select 用 value 作为选项值（不是 key）
    value: entry.tag_name,
  })),
)
watch(() => props.repo?.id, () => { selectedTag.value = null })
// 仓库首次加载/刷新出 Release 后，把选中版本落到最新一个（否则下拉显示空）
watch(
  () => props.repo?.cached_releases,
  (list) => {
    if (!list?.length) return
    const exists = list.some((entry) => entry.tag_name === selectedTag.value)
    if (!exists) selectedTag.value = list[0].tag_name
  },
  { immediate: true },
)

/** 以下指标全部跟随「当前选中的版本」，切换下拉时同步变化。 */
const version = computed(() => selectedRelease.value?.version || selectedRelease.value?.tag_name || '—')

/** 资产规则匹配（exact / glob / regex），与后端 asset_rules 语义一致。 */
function assetMatches(name: string, rule: { type: string; value: string }): boolean {
  if (rule.type === 'exact') return name.toLocaleLowerCase() === rule.value.toLocaleLowerCase()
  if (rule.type === 'glob') {
    const escaped = rule.value.replace(/[.+^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*').replace(/\?/g, '.')
    try { return new RegExp(`^${escaped}$`, 'i').test(name) } catch { return false }
  }
  try { return new RegExp(rule.value, 'i').test(name) } catch { return false }
}

/** 命中资产规则的 Release 资产列表。 */
const matchedAssets = computed(() => {
  const repo = props.repo
  const target = selectedRelease.value
  if (!repo || !target || repo.download_mode === 'repository_file') return []
  return target.assets.filter((asset) =>
    !!asset.download_url && repo.asset_rules.some((rule) => assetMatches(asset.name, rule)),
  )
})
/** 当前选中的资产（默认第一个匹配文件）。 */
const selectedAssetName = ref<string | null>(null)
const assetOptions = computed(() =>
  // Naive UI 的 n-select 用 value 作为选项值（不是 key）
  matchedAssets.value.map((asset) => ({ label: asset.name, value: asset.name })),
)
const primaryAsset = computed(() =>
  matchedAssets.value.find((asset) => asset.name === selectedAssetName.value) ?? matchedAssets.value[0] ?? null,
)
// 切换版本/仓库时清掉已选资产
watch([() => props.repo?.id, selectedTag], () => { selectedAssetName.value = null })

function formatSize(bytes: number | undefined) {
  if (!bytes) return '—'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let index = 0
  while (value >= 1024 && index < units.length - 1) { value /= 1024; index += 1 }
  return `${value.toFixed(index ? 1 : 0)} ${units[index]}`
}

const releaseUrl = computed(() => {
  const repo = props.repo
  if (!repo) return ''
  return repo.provider === 'gitee'
    ? `https://gitee.com/${repo.username}/${repo.repository}/releases/latest`
    : `https://github.com/${repo.username}/${repo.repository}/releases/latest`
})
const sourceUrl = computed(() => {
  const repo = props.repo
  if (!repo) return ''
  return repo.provider === 'gitee'
    ? `https://gitee.com/${repo.username}/${repo.repository}`
    : `https://github.com/${repo.username}/${repo.repository}`
})
/** Raw 地址：指向仓库的文件列表（release 模式）或具体文件（repository_file 模式）。 */
const rawUrl = computed(() => {
  const repo = props.repo
  if (!repo) return ''
  if (repo.download_mode === 'repository_file' && repo.branch && repo.file_path) {
    return repo.provider === 'gitee'
      ? `https://gitee.com/${repo.username}/${repo.repository}/raw/${repo.branch}/${repo.file_path}`
      : `https://github.com/${repo.username}/${repo.repository}/raw/refs/heads/${repo.branch}/${repo.file_path}`
  }
  // 没有具体文件时指向仓库根目录的 Raw 浏览入口
  return repo.provider === 'gitee'
    ? `https://gitee.com/${repo.username}/${repo.repository}/raw/${repo.branch || 'master'}`
    : `https://github.com/${repo.username}/${repo.repository}/raw/refs/heads/${repo.branch || 'main'}`
})
const html = computed(() => renderReadme(readme.value?.content ?? ''))
/** 备注同样按 Markdown 渲染（note_i18n 已按当前语言取值）。 */
const noteHtml = computed(() => renderReadme(note.value))
/** 当前选中 Release 的提交日志（Markdown 渲染，与 note/README 同一函数）。 */
const logHtml = computed(() => renderReadme(selectedRelease.value?.body?.trim() ?? ''))

/**
 * README 区域是否显示：加载中、请求失败、有内容时都显示；
 * 只有确认「文件不存在」时整块隐藏（连同上方的分割线）。
 */
const readmeSectionVisible = computed(() => {
  if (loading.value) return true
  if (error.value) return true
  return !!readme.value?.exists
})

/** Markdown 容器：v-html 更新后把 mermaid 占位块渲染成 SVG。 */
const noteElement = ref<HTMLElement | null>(null)
const readmeElement = ref<HTMLElement | null>(null)
const logElement = ref<HTMLElement | null>(null)
watch(
  [noteHtml, logHtml, readmeElement, loading],
  async () => {
    await nextTick()
    void renderMermaidBlocks(noteElement.value)
    void renderMermaidBlocks(logElement.value)
    void renderMermaidBlocks(readmeElement.value)
  },
  { flush: 'post' },
)

async function load() {
  const repo = props.repo
  cancel()
  if (!repo) return
  const id = crypto.randomUUID()
  requestId = id
  loading.value = true
  error.value = ''
  readme.value = null
  try {
    const result = await api.repositoryReadme(repo.id, id)
    if (requestId === id) readme.value = result
  } catch (reason) {
    if (requestId === id) error.value = reason instanceof Error ? reason.message : String(reason)
  } finally {
    if (requestId === id) { loading.value = false; requestId = null }
  }
}

function cancel() {
  if (requestId) void api.cancelRepositoryReadme(requestId).catch(() => undefined)
  requestId = null
  loading.value = false
}

watch(() => props.repo?.id, () => {
  // 切换仓库时回到「说明」页，避免上一个仓库停在下载安装页
  activeTab.value = 'info'
  void load()
}, { immediate: true })
onUnmounted(cancel)

async function openUrl(url: string) {
  if (!url) return
  await api.openReleasePage(url).catch(() => undefined)
}
</script>

<template>
  <div v-if="repo" class="detail">
    <header class="detail-head">
      <div class="head-main">
        <span class="tile">{{ (name[0] || '?').toUpperCase() }}</span>
        <div class="head-text">
          <b>{{ name }}</b>
          <!-- 渠道 · 用户/仓库 · Release/Raw：后两段可点 -->
          <small class="head-meta">
            <span class="meta-channel">{{ repo.provider === 'gitee' ? 'Gitee' : 'GitHub' }}</span>
            <span class="meta-sep">·</span>
            <button type="button" class="meta-link" :title="sourceUrl" @click="openUrl(sourceUrl)">
              {{ repo.username }}/{{ repo.repository }}
            </button>
            <span class="meta-sep">·</span>
            <!-- 按仓库下载模式二选一：release 模式给 Release 页，仓库文件模式给 Raw 文件 -->
            <template v-if="repo.download_mode === 'release'">
              <button type="button" class="meta-link" :title="releaseUrl" @click="openUrl(releaseUrl)">Release</button>
            </template>
            <template v-else>
              <button type="button" class="meta-link" :title="rawUrl" @click="openUrl(rawUrl)">Raw</button>
            </template>
          </small>
        </div>
      </div>
      <!-- 关闭按钮由抽屉自带的关闭入口负责，这里不再重复放一个叉 -->
    </header>

    <!-- 分段切换：说明（备注 + README）/ 下载安装（版本 / 日志 / 资产 / 安装） -->
    <n-tabs v-model:value="activeTab" type="segment" animated class="detail-tabs">
      <n-tab-pane name="info" :tab="t('说明')">
        <section v-if="note" ref="noteElement" class="note markdown-body" v-html="noteHtml"></section>
        <section v-else class="note muted">{{ t('该仓库没有备注') }}</section>

        <!-- README：加载中显示骨架动画；文件不存在时整块（含分割线）收起 -->
        <Transition name="readme-collapse">
          <div v-if="readmeSectionVisible" class="readme-section">
            <n-divider />

            <section class="readme">
              <Transition name="readme-content" mode="out-in">
                <div v-if="loading" key="loading" class="readme-loading" role="status"
                  :aria-label="t('正在加载仓库说明')">
                  <!-- 宽度与圆角统一在 .readme-loading :deep(.n-skeleton) 里定义 -->
                  <n-skeleton text class="skeleton-line" />
                  <n-skeleton text class="skeleton-line" />
                  <n-skeleton text class="skeleton-line" />
                  <n-skeleton text class="skeleton-line" />
                </div>
                <div v-else-if="error" key="error" class="muted">{{ error }}</div>
                <div v-else key="body" ref="readmeElement" class="markdown-body" v-html="html"></div>
              </Transition>
            </section>
          </div>
        </Transition>
      </n-tab-pane>

      <!-- 面板 2：下载安装（版本选择 / 提交日志 / 资产与安装） -->
      <n-tab-pane name="install" :tab="t('下载安装')">
        <!-- 指标 + 安装合并区：版本下拉 / 版本绑定 / 资产信息 / 安装 -->
        <div class="summary">
          <div class="summary-row">
            <div class="metric metric-version">
              <span class="muted">{{ t('选择版本') }}</span>
              <n-select v-if="releaseOptions.length" v-model:value="selectedTag" class="version-select" size="small"
                :options="releaseOptions" :placeholder="t('选择版本')" />
              <b v-else class="mono">{{ version }}</b>
            </div>
            <div class="metric metric-id">
              <span class="muted">{{ t('版本绑定') }}</span>
              <b class="mono">{{ repo.username }}/{{ repo.repository }}</b>
            </div>
          </div>

          <!-- 第二段：当前版本的提交日志（独立滚动，最大高度固定） -->
          <div class="release-log">
            <div class="log-head">
              <span class="muted">{{ t('提交日志') }}</span>
              <b class="mono">{{ selectedRelease?.name || selectedRelease?.tag_name || '—' }}</b>
              <span v-if="selectedRelease?.published_at" class="muted">
                {{ selectedRelease.published_at.slice(0, 10) }}
              </span>
              <n-tag v-if="selectedRelease?.prerelease" size="tiny" type="warning" :bordered="false">{{ t('预发布') }}</n-tag>
            </div>
            <div v-if="selectedRelease?.body?.trim()" ref="logElement" class="log-box markdown-body" v-html="logHtml"></div>
            <div v-else class="log-box log-empty muted">{{ t('该版本没有提交日志') }}</div>
          </div>

          <!-- 资产选择 / 大小与下载次数 / 安装按钮 -->
          <div class="install-row">
            <!-- 左侧：可选已识别的 Release 资产（或仓库文件模式的文件） -->
            <n-select v-if="repo.download_mode === 'release' && assetOptions.length" class="asset-select" size="small"
              :value="primaryAsset?.name ?? null" :options="assetOptions" :placeholder="t('选择文件')"
              @update:value="(value: string) => (selectedAssetName = value)" />
            <span v-else-if="repo.download_mode === 'repository_file'" class="asset-select-file mono">
              {{ repo.branch }}/{{ repo.file_path }}
            </span>
            <span v-else class="asset-select-file muted">{{ t('该版本没有命中资产规则的文件') }}</span>

            <!-- 中右：大小与下载次数 -->
            <div class="asset-stats">
              <template v-if="repo.download_mode === 'repository_file'">
                <span class="muted">{{ t('仓库文件模式直接下载该文件') }}</span>
              </template>
              <template v-else-if="primaryAsset">
                <span class="stat-item">
                  <span class="muted">{{ t('大小') }}</span>
                  <b>{{ formatSize(primaryAsset.size) }}</b>
                </span>
                <span class="stat-item">
                  <span class="muted">{{ t('下载量') }}</span>
                  <b>{{ (primaryAsset.download_count ?? 0).toLocaleString() }}</b>
                </span>
              </template>
            </div>

            <!-- 最右：安装 -->
            <n-button type="primary" :loading="installing"
              :disabled="gameBlocked || (repo.download_mode === 'release' && !primaryAsset)"
              @click="emit('install', repo.download_mode === 'release' ? selectedRelease?.tag_name : undefined)">
              <DownloadOutline width="15" height="15" />{{ t('安装') }}
            </n-button>
          </div>

          <!-- 仓库文件模式的下载方式已由安装行表达，这里只保留未启用提示 -->
          <div v-if="repo.download_mode !== 'repository_file' && !repo.enabled" class="summary-tags">
            <n-tag size="small" :bordered="false">{{ t('未启用') }}</n-tag>
          </div>
        </div>
      </n-tab-pane>
    </n-tabs>
  </div>
</template>

<style scoped>
.detail {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding: 16px 18px 20px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 14px;
  background: var(--oc2-card);
  /* 面板切换由 n-tabs 自己裁剪，这里只兜底裁掉圆角外的内容 */
  overflow: clip;
}
/*
 * 分段切换用 Naive 的 n-tabs type="segment"：
 * 撑满容器宽度、选中项自动使用主题色，这里只补一处间距。
 */
.detail-tabs :deep(.n-tabs-nav) { margin-bottom: 2px; }
/* 面板内部保持纵向间距；Naive 的默认 pane padding 由它自己的变量控制 */
.detail-tabs :deep(.n-tab-pane) {
  display: flex;
  flex-direction: column;
  gap: 14px;
  padding-top: 12px;
}
.detail-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
}
.head-main { display: flex; align-items: center; gap: 12px; min-width: 0; }
.tile {
  width: 44px;
  height: 44px;
  flex: 0 0 44px;
  border-radius: 12px;
  display: grid;
  place-items: center;
  font-size: 18px;
  font-weight: 700;
  color: #fff;
  background: linear-gradient(135deg, var(--oc2-accent), var(--oc2-accent-hover));
}
.head-text { display: flex; flex-direction: column; gap: 3px; min-width: 0; }
.head-text b { font-size: 16px; color: var(--oc2-text-1); }
/* 渠道 · 用户/仓库 · Release/Raw */
.head-meta {
  display: flex;
  align-items: center;
  gap: 4px;
  flex-wrap: wrap;
  font-size: 12px;
  color: var(--oc2-text-3);
  min-width: 0;
}
.meta-channel { flex: 0 0 auto; }
.meta-sep { flex: 0 0 auto; color: var(--oc2-text-4); }
.meta-link {
  border: 0;
  padding: 0;
  background: none;
  color: var(--oc2-text-2);
  font: inherit;
  cursor: pointer;
  max-width: 240px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  transition: color .16s ease;
}
.meta-link:hover { color: var(--oc2-accent); text-decoration: underline; }
.meta-link:focus-visible { outline: 1px solid var(--oc2-accent); outline-offset: 2px; border-radius: 3px; }

/* 指标 + 安装合并区 */
.summary {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  background: var(--oc2-sunken);
}
.summary-row {
  /* 版本列按内容自适应（不挤压版本号），版本绑定吃掉剩余空间 */
  display: grid;
  grid-template-columns: minmax(120px, max-content) minmax(0, 1fr);
  gap: 12px;
  align-items: start;
}
/* 第二段：提交日志（独立滚动区域，最大高度固定） */
.release-log {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-top: 8px;
  border-top: 1px solid var(--oc2-row-line);
}
.log-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
  min-width: 0;
}
.log-head b {
  font-size: 12.5px;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 100%;
}
.log-box {
  max-height: 180px;
  overflow-y: auto;
  padding: 8px 10px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 8px;
  background: var(--oc2-card);
  font-size: 12px;
  line-height: 1.65;
  overscroll-behavior: contain;
}
.log-box.log-empty {
  display: grid;
  place-items: center;
  min-height: 56px;
  font-size: 12px;
}
.metric {
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}
.metric b {
  font-size: 13px;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.metric-version {
  /* 版本列只有 1/6 宽：下拉撑满该列，靠省略号处理长版本号 */
  display: flex;
  flex-direction: column;
  gap: 3px;
  min-width: 0;
}
.version-select {
  /* 宽度随所选版本号内容自适应（不截断版本号），列宽也由内容决定 */
  display: inline-flex !important;
  width: auto !important;
  min-width: 0;
}
.version-select :deep(.n-base-selection) {
  min-width: 0;
}
.version-select :deep(.n-base-selection-label) {
  padding-right: 24px;
}
/*
 * 下拉框后缀：Naive 用 n-base-loading 包装箭头，未 loading 时里面是
 * placeholder。这里只保留箭头、隐藏多余的占位层，避免出现看似转圈的圆。
 */
.version-select :deep(.n-base-loading),
.asset-select :deep(.n-base-loading) {
  width: 16px;
  height: 16px;
}
.version-select :deep(.n-base-loading__placeholder),
.asset-select :deep(.n-base-loading__placeholder) {
  display: block;
}
.version-select :deep(.n-base-selection-input) {
  min-width: 0;
  overflow: visible;
}
.version-select :deep(.n-base-selection-input__content) {
  /* 不省略：版本号完整显示，宽度由内容撑开 */
  overflow: visible;
  text-overflow: clip;
  white-space: nowrap;
}
.metric-id b { font-size: 11.5px; }
.summary-tags { display: flex; gap: 6px; flex-wrap: wrap; }
/* 安装行：资产下拉 / 大小与下载次数 / 安装按钮 */
.install-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding-top: 8px;
  border-top: 1px solid var(--oc2-row-line);
}
.asset-select {
  /* 下拉吸收多余水平空间，统计项保持紧凑贴靠安装按钮 */
  flex: 1 1 240px;
  min-width: 140px;
  max-width: 420px;
}
.asset-select-file {
  flex: 1 1 240px;
  min-width: 0;
  max-width: 420px;
  font-size: 12px;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.asset-stats {
  /* 统计项自然宽度，整体靠右紧贴安装按钮；多余空隙落在下拉与统计之间 */
  flex: 0 0 auto;
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 18px;
  font-size: 12px;
}
.stat-item {
  display: flex;
  align-items: baseline;
  gap: 5px;
  min-width: 0;
}
.stat-item b {
  color: var(--oc2-text-1);
  font-size: 12.5px;
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
}
/* 分隔线由 Naive 的 n-divider 负责，这里只收紧它的上下间距 */
.detail-tabs :deep(.n-divider) { margin: 2px 0; }
/* 备注区与 README 用同一套 Markdown 样式，这里只补外层间距。 */
.note {
  margin: 0;
}

/* README：内容决定高度，不额外留底部空白（骨架态自己撑高） */
.readme { margin: 0; padding: 0; }
/* README 整块（含分割线）在「文件不存在」时平滑收起，而不是瞬间消失 */
.readme-section {
  overflow: hidden;
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.readme-collapse-enter-active,
.readme-collapse-leave-active {
  overflow: hidden;
  transition: max-height .26s cubic-bezier(.4, 0, .2, 1), opacity .2s ease;
  max-height: 4000px;
  opacity: 1;
}
.readme-collapse-enter-from,
.readme-collapse-leave-to {
  max-height: 0;
  opacity: 0;
}
/* 内容切换（骨架 → 正文 / 错误）：向下展开与向上收起走同一组参数，保持对称 */
.readme-content-enter-active,
.readme-content-leave-active {
  transition: opacity .18s ease, transform .18s ease;
}
.readme-content-enter-from {
  opacity: 0;
  transform: translateY(-6px);
}
.readme-content-leave-to {
  opacity: 0;
  transform: translateY(6px);
}
@media (prefers-reduced-motion: reduce) {
  .readme-content-enter-active,
  .readme-content-leave-active { transition: none; }
}
@media (prefers-reduced-motion: reduce) {
  .readme-collapse-enter-active,
  .readme-collapse-leave-active { transition: none; }
}
/* 加载骨架：用 Naive 的 n-skeleton，这里只保留纵向间距 */
/*
 * 加载骨架：高度由内容自己决定，不再预留 min-height。
 * 旧版是 1em 高的满宽矩形，为了不让 README 区域在加载完成时塌缩才垫了
 * 88px；现在骨架条本身只有 9px 高，继续留 88px 会在动画下方空出一大段。
 */
.readme-loading {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
/*
 * 骨架条：Naive 的 n-skeleton 默认是 1em 高的实心矩形，满宽一条很抢眼。
 * 改成圆角长条并缩窄到原宽度的 2/3 左右 —— 骨架只是占位，
 * 不该比真实内容更醒目；长短交错读起来像段落而不是几个色块。
 */
.readme-loading :deep(.n-skeleton) {
  height: 9px;
  border-radius: 999px;
}
.readme-loading :deep(.skeleton-line:nth-child(1)) { width: 66%; }
.readme-loading :deep(.skeleton-line:nth-child(2)) { width: 58%; }
.readme-loading :deep(.skeleton-line:nth-child(3)) { width: 66%; }
.readme-loading :deep(.skeleton-line:nth-child(4)) { width: 42%; }

/* README 内容 */
.markdown-body { font-size: 12.5px; line-height: 1.7; color: var(--oc2-text-2); overflow-wrap: anywhere; }
.markdown-body :deep(h1),
.markdown-body :deep(h2),
.markdown-body :deep(h3),
.markdown-body :deep(h4) {
  margin: 16px 0 8px;
  color: var(--oc2-text-1);
  line-height: 1.35;
}
.markdown-body :deep(h1) { font-size: 17px; }
.markdown-body :deep(h2) { font-size: 15px; }
.markdown-body :deep(h3) { font-size: 13.5px; }
/* README 首个元素（通常是标题）不留顶部外边距，避免内容和分割线之间出现空隙 */
.readme-section :deep(.markdown-body > :first-child) { margin-top: 0; }
.markdown-body :deep(p) { margin: 0 0 10px; }
/* 末元素不留底部外边距，避免 README/备注区域底部出现空隙 */
.markdown-body :deep(> :last-child) { margin-bottom: 0; }
.markdown-body :deep(ul),
.markdown-body :deep(ol) { margin: 0 0 10px; padding-left: 22px; }
.markdown-body :deep(li) { margin: 3px 0; }
.markdown-body :deep(code) {
  padding: 1px 5px;
  border-radius: 5px;
  background: var(--oc2-sunken);
  font-family: Consolas, monospace;
  font-size: 11.5px;
  color: var(--oc2-accent);
}
.markdown-body :deep(pre) {
  margin: 0 0 12px;
  padding: 10px 12px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 8px;
  background: var(--oc2-sunken);
  overflow-x: auto;
}
.markdown-body :deep(pre code) { padding: 0; background: none; color: var(--oc2-text-2); }
.markdown-body :deep(blockquote) {
  margin: 0 0 12px;
  padding: 8px 12px;
  border-left: 3px solid var(--oc2-accent);
  background: var(--oc2-sunken);
  color: var(--oc2-text-3);
  border-radius: 0 8px 8px 0;
}
.markdown-body :deep(a) { color: var(--oc2-accent); text-decoration: none; }
.markdown-body :deep(a:hover) { text-decoration: underline; }
.markdown-body :deep(img) { max-width: 100%; border-radius: 8px; }
.markdown-body :deep(hr) { border: 0; border-top: 1px solid var(--oc2-row-line); margin: 14px 0; }
/* Mermaid：渲染前保留代码块，渲染后居中显示 SVG */
.markdown-body :deep(.mermaid-block) {
  margin: 0 0 12px;
  padding: 10px 12px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 8px;
  background: var(--oc2-sunken);
  overflow-x: auto;
}
.markdown-body :deep(.mermaid-block.mermaid-rendered) {
  display: grid;
  place-items: center;
  padding: 14px;
  background: var(--oc2-card);
}
.markdown-body :deep(.mermaid-block.mermaid-rendered svg) {
  max-width: 100%;
  height: auto;
}
.markdown-body :deep(.mermaid-error) {
  border-color: #8c4038;
}
.markdown-body :deep(.mermaid-error-hint) {
  margin: -6px 0 12px;
  color: #ee8585;
  font-size: 11.5px;
  overflow-wrap: anywhere;
}
</style>
