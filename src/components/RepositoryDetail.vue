<script setup lang="ts">
/**
 * MOD 安装页的仓库详情（右侧抽屉的内容）。
 *
 * 版式在 `ReleaseDetailPanel.vue` —— 与地图下载页共用同一套骨架
 * （头部 / 说明·下载安装 tabs / 依赖表 / 版本选择与绑定 / 提交日志 / 资产下拉 / 主按钮）。
 * 这里只留 MOD 侧真正独有的数据：缓存 Release、资产规则匹配、依赖关系判定、
 * 仓库链接地址。
 *
 * README 的拉取与取消在 `useRepositoryReadme`：地图下载页的地图详情要用同一份
 * 实现（后端 `get_repository_readme` 两种 id 都认）。
 */
import { computed, ref, watch } from 'vue'
import { api } from '../services/tauri'
import { pickI18n, t } from '../i18n'
import { formatVersion, isPrereleaseVersion } from '../modStatus'
import { matchedReleaseAssets } from '../repositoryAssets'
import { resolveRelations, type RelationStatus } from '../modRelations'
import { useRepositoryReadme } from '../composables/useRepositoryReadme'
import ReleaseDetailPanel from './ReleaseDetailPanel.vue'
import type { ReleaseDetailLink } from './ReleaseDetailPanel.vue'
import type { DependencyRow } from './DependencyTable.vue'
import type { InstalledMod, Repository } from '../types'

const props = withDefaults(defineProps<{
  repo: Repository | null
  installing?: boolean
  gameBlocked?: boolean
  /**
   * 已装 MOD（依赖表的「本地是否满足」判定对象）。
   *
   * 与安装前检查、卡片上「缺依赖」标签用**同一份** Dashboard 列表：面板里的
   * 依赖表和点安装后弹出的检查弹窗必须给出同一个结论，否则用户会看到
   * 「表里说已满足，弹窗又说缺」。
   */
  mods?: readonly InstalledMod[]
  /** 可安装来源（缺依赖时它从哪个仓库补；与安装前检查同一个列表）。 */
  repositories?: readonly Repository[]
}>(), {
  installing: false,
  gameBlocked: false,
  mods: () => [],
  repositories: () => [],
})
/** install 携带选中的 tag 与资产（未选则用最新 / 按资产规则自动选）。 */
const emit = defineEmits<{ close: []; install: [tag?: string, asset?: string]; back: [] }>()

const name = computed(() => (props.repo ? pickI18n(props.repo.name_i18n, props.repo.name) : ''))
const note = computed(() => (props.repo ? pickI18n(props.repo.note_i18n, props.repo.note || '') : ''))

const { readme, loading, error } = useRepositoryReadme(() => props.repo?.id ?? null)

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
    // 显示用 tag_name（原始写法，含第 4 位）；version 是 semver 归一值会丢段
    label: formatVersion(entry.tag_name || entry.version || ''),
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

/**
 * 无下拉时的版本文本，跟随「当前选中的版本」。
 *
 * 版本号优先取 `tag_name`：后端 `version` 是 semver 归一结果
 * （`1.8.7.23` → `1.8.7`），直接显示会丢掉第 4 位。
 */
const version = computed(() =>
  formatVersion(selectedRelease.value?.tag_name || selectedRelease.value?.version || '') || '—',
)

/** 命中资产规则的 Release 资产列表（匹配实现与地图共用 `repositoryAssets.ts`）。 */
const matchedAssets = computed(() => {
  const repo = props.repo
  const target = selectedRelease.value
  if (!repo || !target || repo.download_mode === 'repository_file') return []
  return matchedReleaseAssets(target, repo.asset_rules)
})
/** 当前选中的资产（默认第一个匹配文件）。 */
const selectedAssetName = ref<string | null>(null)
const primaryAsset = computed(() =>
  matchedAssets.value.find((asset) => asset.name === selectedAssetName.value) ?? matchedAssets.value[0] ?? null,
)
// 切换版本/仓库时清掉已选资产
watch([() => props.repo?.id, selectedTag], () => { selectedAssetName.value = null })

/* ===== 依赖表 ===== */

/**
 * 版本区间的展示文本。
 *
 * 只写一端时用单边符号；两端都有时写成「区间」而不是 `≥x, ≤y` —— 后者
 * 跟到表格里太长，一眼看不出是闭区间。
 */
function requirementText(status: RelationStatus): string {
  const min = status.relation.min_version?.trim() ?? ''
  const max = status.relation.max_version?.trim() ?? ''
  if (min && max) return `${min} ~ ${max}`
  if (min) return `≥ ${min}`
  if (max) return `≤ ${max}`
  return ''
}

/** 依赖目标的可读名：优先可安装来源的本地化名，其次本地 MOD 名，最后 GUID。 */
function dependencyName(status: RelationStatus): string {
  const target = status.repository
  if (target) return pickI18n(target.name_i18n, target.name) || status.relation.guid
  if (status.installedMod?.name) return status.installedMod.name
  return status.relation.guid
}

/** 状态文案：冲突/兼容与依赖是两套语义，不能共用「已满足」。 */
function dependencyStatus(status: RelationStatus): string {
  const kind = status.relation.kind ?? 'requires'
  if (kind === 'conflicts') {
    return status.state === 'conflict' ? t('modsOnline.conflict') : t('tools.noConflict')
  }
  if (kind === 'compatible') {
    return status.state === 'conflict' ? t('tools.incompatible') : t('tools.compatible')
  }
  if (status.state === 'ok') return t('tools.satisfied')
  return status.state === 'installable' ? t('tools.installable') : t('modsOnline.missingDep')
}

/**
 * 依赖表行。
 *
 * 判定走 `resolveRelations`（与安装前检查、卡片上的「缺依赖」标签同一份
 * 实现），版本按**当前选中的 Release** 传：仓库可以按版本区间声明不同关系
 * （`release_relations`），拿最新版的关系去解释旧版本会给出错误结论。
 */
const dependencyRows = computed<DependencyRow[]>(() => {
  const repo = props.repo
  if (!repo) return []
  const statuses = resolveRelations(repo, props.mods, props.repositories, selectedRelease.value?.tag_name)
  return statuses.map((status) => ({
    key: `${status.relation.guid}:${status.relation.kind ?? 'requires'}`,
    name: dependencyName(status),
    // 关系名照原样填：表格只在出现非 requires 关系时才显示这一列，
    // 那一列里 requires 行也要有自己的名字，不能留 `—`
    kind: status.relation.kind ?? 'requires',
    requirement: requirementText(status),
    status: dependencyStatus(status),
    tone: status.state === 'ok'
      ? 'mild' as const
      : status.state === 'installable' ? 'moderate' as const : 'critical' as const,
  }))
})

/** 仓库文件模式：安装行左侧直接显示 `branch/file_path`。 */
const fileLabel = computed(() => {
  const repo = props.repo
  if (!repo || repo.download_mode !== 'repository_file') return ''
  return `${repo.branch}/${repo.file_path}`
})

/** 仓库被禁用时的标签（面板只在有值时渲染）。 */
const disabledLabel = computed(() => {
  const repo = props.repo
  if (!repo) return ''
  return repo.download_mode !== 'repository_file' && !repo.enabled ? t('modsOnline.disabled') : ''
})

/* ===== 链接 ===== */
const sourceUrl = computed(() => {
  const repo = props.repo
  if (!repo) return ''
  return repo.provider === 'gitee'
    ? `https://gitee.com/${repo.username}/${repo.repository}`
    : `https://github.com/${repo.username}/${repo.repository}`
})
const releaseUrl = computed(() => (sourceUrl.value ? `${sourceUrl.value}/releases/latest` : ''))
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
/** 头部第三段链接：release 模式给 Release 页，仓库文件模式给 Raw 文件。 */
const metaLinks = computed<ReleaseDetailLink[]>(() => {
  const repo = props.repo
  if (!repo) return []
  return repo.download_mode === 'release'
    ? [{ label: 'Release', url: releaseUrl.value }]
    : [{ label: 'Raw', url: rawUrl.value }]
})

/* ===== 提交日志 / 预发布 ===== */
const releaseNotes = computed(() => selectedRelease.value?.body ?? '')
/**
 * 选中版本是否预发布：元数据标记 **或** 版本号带 `-` 后缀。
 * 作者没勾 prerelease 但版本叫 0.4.0-beta 时，同样要提示。
 */
const selectedReleaseIsPrerelease = computed(() => {
  const release = selectedRelease.value
  if (!release) return false
  // 传 tag_name：version 是后端归一后的三段，第 4 段 beta 序号已经丢了
  return isPrereleaseVersion(release.tag_name || release.version, release.prerelease)
})

async function openUrl(url: string) {
  if (!url) return
  await api.openReleasePage(url).catch(() => undefined)
}
</script>

<template>
  <!-- `:key` 让切换仓库时重建面板：分段选择回到「说明」页，资产/版本下拉状态一起重置 -->
  <ReleaseDetailPanel v-if="repo" :key="repo.id"
    :title="name" :provider="repo.provider" :owner="repo.username" :repository="repo.repository"
    :meta-links="metaLinks" :note="note"
    :readme-content="readme?.content ?? ''" :readme-exists="!!readme?.exists"
    :readme-loading="loading" :readme-error="error"
    :dependencies="dependencyRows"
    :versions="releaseOptions" :version="selectedRelease?.tag_name ?? null" :version-fallback="version"
    :binding-value="`${repo.username}/${repo.repository}`"
    :release-tag="selectedRelease?.tag_name || selectedRelease?.name || '—'"
    :published-at="selectedRelease?.published_at ?? null" :prerelease="selectedReleaseIsPrerelease"
    :release-notes="releaseNotes"
    :assets="matchedAssets" :asset-value="primaryAsset?.name ?? null" :file-label="fileLabel"
    :disabled-label="disabledLabel" :installing="installing" :game-blocked="gameBlocked"
    @update:version="(value: string) => (selectedTag = value)"
    @update:asset="(value: string) => (selectedAssetName = value)"
    @open-link="(url: string) => { void openUrl(url) }"
    @install="(tag?: string, asset?: string) => emit('install', tag, asset)" />
</template>
