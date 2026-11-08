<script setup lang="ts">
import { repositoryHomepage, repositoryRawUrl } from '../providers'
/**
 * MOD 安装页的仓库详情（右侧抽屉的内容）。
 *
 * 版式在 `ReleaseDetailPanel.vue` —— 与地图下载页共用同一套骨架
 * （头部 / 说明·下载安装 tabs / 依赖表 / 版本选择与绑定 / 提交日志 / 资产下拉 / 主按钮）。
 * 这里只留 MOD 侧真正独有的数据：缓存 Release、资产规则匹配、依赖关系判定、
 * 仓库链接地址。
 *
 * README 的拉取与取消在 `useRepositoryReadme`：只有 MOD 详情需要它。地图详情
 * 用条目自己的 `note_i18n` 当说明，仓库 README 不参与（仓库级内容会串味）。
 */
import { computed, ref, watch } from 'vue'
import { api } from '../services/tauri'
import { pickI18n, t } from '../i18n'
import { formatVersion, isPrereleaseVersion } from '../modStatus'
import { matchedReleaseAssets } from '../repositoryAssets'
import { relationsForRelease } from '../modRelations'
import { dependencyRowsFor } from '../dependencyRows'
import { useRepositoryLink } from '../crossLink'
import { useRepositoryReadme } from '../composables/useRepositoryReadme'
import ReleaseDetailPanel from './ReleaseDetailPanel.vue'
import type { ReleaseDetailLink } from './ReleaseDetailPanel.vue'
import type { DependencyRow } from '../dependencyRows'
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

/** 依赖表里的跳转：与卡片上的「缺依赖」标签同一个入口（安装页 + `focus` 高亮）。 */
const { linkToRepository } = useRepositoryLink()

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
 * 依赖表行。
 *
 * 判定与取名都走 `dependencyRows.ts`（四个页面共用一份口径，见那里的注释）：
 * 这片面板只负责把「当前选中的 Release」和 Dashboard 的两份数据递进去。
 *
 * 版本按**当前选中的 Release** 传：仓库可以按版本区间声明不同关系
 * （`release_relations`），拿最新版的关系去解释旧版本会给出错误结论。
 */
const dependencyRows = computed<DependencyRow[]>(() => {
  const repo = props.repo
  if (!repo) return []
  return dependencyRowsFor(relationsForRelease(repo, selectedRelease.value?.tag_name), {
    mods: props.mods,
    repositories: props.repositories,
    ownerRepository: repo,
    translate: t,
    pickName: pickI18n,
    jumpToRepository: true,
  })
})

/**
 * 依赖表里点「可安装 / 可更新」：先关抽屉，再跳到安装页那张仓库卡片。
 *
 * 抽屉是右侧浮层，不关掉的话目标卡片被遮罩盖住，用户会以为点了没反应。
 */
function openDependencyJump(row: DependencyRow) {
  const id = row.jump?.id
  if (!id) return
  emit('close')
  linkToRepository(id)
}

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
  return repositoryHomepage(repo.provider, repo.username, repo.repository)
})
const releaseUrl = computed(() => (sourceUrl.value ? `${sourceUrl.value}/releases/latest` : ''))
/** Raw 地址：指向仓库的文件列表（release 模式）或具体文件（repository_file 模式）。 */
const rawUrl = computed(() => {
  const repo = props.repo
  if (!repo) return ''
  return repositoryRawUrl(repo.provider, repo.username, repo.repository, repo.branch || (repo.provider === 'gitee' ? 'master' : 'main'), repo.download_mode === 'repository_file' ? repo.file_path || '' : '')
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
    @dependency-jump="openDependencyJump"
    @close="emit('close')"
    @install="(tag?: string, asset?: string) => emit('install', tag, asset)" />
</template>
