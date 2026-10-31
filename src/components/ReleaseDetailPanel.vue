<script lang="ts">
/**
 * 共享「发布详情面板」的对外类型。
 *
 * 面板**不认识** `Repository` / `MapInfo` 任何一方：MOD 安装页和地图下载页的
 * 数据结构、状态机、安装命令都不一样，唯一相同的是「同一个 Release 长什么样」。
 * 所以这里的类型只描述面板真正会读到的字段，调用方传入自己的对象即可
 * （结构满足就能过类型检查）。
 */

/** 面板能渲染的最小资产结构（MOD 的 `ReleaseAsset` 与地图资产都满足）。 */
export interface ReleaseDetailAsset {
  name: string
  size?: number | null
  download_count?: number | null
}

/** 头部第二行的链接：仓库主页 / Release / Raw。 */
export interface ReleaseDetailLink {
  label: string
  url: string
}

/** 版本下拉选项。 */
export interface ReleaseDetailVersion {
  label: string
  value: string
}
</script>

<script setup lang="ts">
/**
 * 发布详情面板 —— MOD 安装页与地图下载页共用的**版式骨架**。
 *
 * ## 为什么要有这层
 *
 *「同一个 Release」在两个页面曾经各画一遍：MOD 侧是 `RepositoryDetail.vue` 的
 * 分段 tabs（说明 / 下载安装）+ 版本下拉 + 提交日志 + 资产行 + 安装按钮；
 * 地图侧 `MapDetail.vue` 是扁平 section 拼贴，资产行只有名字和大小。
 * 同一个仓库点进去，两边长得完全不一样 —— 这个组件就是那条共享的骨架：
 * 头部、tabs、版本选择、版本绑定、提交日志（带日期）、资产区、主按钮。
 *
 * ## 边界：只做版式，不做数据
 *
 * 面板不请求任何数据、不算任何业务状态。README 拉取、Release 缓存、资产规则
 * 匹配、安装/刷新命令都在调用方；面板只接收「已经算好的展示值」，并发出
 * `install` / `refresh` / `update:version` / `update:asset` / `open-link`。
 *
 * ## 模块差异怎么表达
 *
 * - **说明区**：`note` 是纯 Markdown 源；`readme-*` 一组是仓库 README，
 *   两个页面都拉（后端 `get_repository_readme` 同时认 MOD 仓库与地图仓库的
 *   id），拉不到就整块收起。
 * - **依赖表**：`dependencies` 由调用方算好（地图是 `required_frameworks`，
 *   MOD 是 `relations`），固定排在**说明 tab 的备注上方**，与备注之间用
 *   分割线隔开。为空时整块不渲染。
 * - **版本绑定**：`binding-value` 留空则整个第二列不渲染（地图没有插件绑定）。
 * - **资产区**：两个页面都是同一个下拉选择（`assets` + `assetValue`）——
 *   地图的资产规则在后端与前端各有一份镜像判定（`repositoryAssets.ts`），
 *   所以「下拉里列的」= 「实际会下载的」。
 * - **主按钮**：`primary-action="refresh"` 用于没有缓存 Release 的地图
 *   （直接安装会命中后端 `releaseMissing`）。
 *
 * 任何可选数据缺失都只是「那块不渲染」，不会有空白占位或塌版。
 */
import { computed, nextTick, ref, watch } from 'vue'
import { NButton, NDivider, NSelect, NSkeleton, NTabPane, NTabs, NTag } from 'naive-ui'
import type { SelectOption } from 'naive-ui'
import { DownloadOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { renderMermaidBlocks, renderReadme } from '../markdown'
import { formatFileSize } from '../repositoryAssets'
import DependencyTable, { type DependencyRow } from './DependencyTable.vue'
import ProviderIcon from './ProviderIcon.vue'

const props = withDefaults(
  defineProps<{
    /* ===== 头部 ===== */
    /** 显示名（仓库名 / 地图名，调用方已按当前语言取好）。 */
    title: string
    provider: 'github' | 'gitee'
    /** 仓库 owner（地图是地图源 owner）。 */
    owner: string
    repository: string
    /** 图标尺寸：MOD 安装页 38，地图下载页 34。 */
    iconSize?: number
    /** 渠道名之后的链接（顺序即展示顺序）。 */
    metaLinks?: ReleaseDetailLink[]

    /* ===== 说明 tab ===== */
    /** 备注（Markdown 源）。空串时显示 `noteEmptyText`。 */
    note?: string
    /** 没有备注时的灰色提示；传空串可整块不显示。 */
    noteEmptyText?: string
    /** README（Markdown 源）；空串 + 不加载 + 无错误 = 整块收起。 */
    readmeContent?: string
    /** README 文件是否确认存在（false + 不加载 + 无错误 = 整块收起）。 */
    readmeExists?: boolean
    readmeLoading?: boolean
    readmeError?: string
    /** 依赖声明（调用方算好并翻译）；固定排在备注上方，为空则整块不渲染。 */
    dependencies?: DependencyRow[]
    /** 依赖表的标题；留空用「依赖」。 */
    dependenciesTitle?: string
    /** 依赖目标名用等宽字体（地图声明的是 GUID；MOD 是仓库名，保持正文字体）。 */
    dependenciesMonoName?: boolean

    /* ===== 下载安装 tab：版本 ===== */
    /** 版本下拉选项；为空时退化成纯文本（`versionFallback`）。 */
    versions?: ReleaseDetailVersion[]
    /** 当前选中的版本值（配合 `update:version`）。 */
    version?: string | null
    /** 没有下拉选项时显示的版本文本。 */
    versionFallback?: string
    /** 「版本绑定」文案；值见 `bindingValue`。 */
    bindingLabel?: string
    /** 「版本绑定」的值；留空则整列不渲染。 */
    bindingValue?: string

    /* ===== 提交日志 ===== */
    /** 日志头部显示的 tag / 名称。 */
    releaseTag?: string
    /** 发布时间（`ISO` 字符串，面板只取前 10 位日期）。 */
    publishedAt?: string | null
    /** 预发布标记（元数据标记或版本号带 `-` 后缀，由调用方判定）。 */
    prerelease?: boolean
    /** 提交日志（Markdown 源）。 */
    releaseNotes?: string
    /** 没有提交日志时的提示。 */
    logEmptyText?: string

    /* ===== 资产与主按钮 ===== */
    /** 可下载资产（两个页面都只放「会被安装的那个」——命中资产规则的文件）。 */
    assets?: ReleaseDetailAsset[]
    /** 下拉里当前选中的资产名（配合 `update:asset`）。 */
    assetValue?: string | null
    /** 仓库文件模式的文件路径（`branch/file_path`）；有值时左侧显示它。 */
    fileLabel?: string
    /** 仓库文件模式下统计区的说明文字。 */
    fileHint?: string
    /** 下拉模式且没有命中资产规则时的提示。 */
    noAssetText?: string
    /** 主按钮动作。 */
    primaryAction?: 'install' | 'refresh'
    /** 附加禁用条件（调用方的业务判断）。 */
    actionDisabled?: boolean
    /** 需要展示的「未启用」小标签；留空不渲染。 */
    disabledLabel?: string
    installing?: boolean
    refreshing?: boolean
    gameBlocked?: boolean
  }>(),
  {
    iconSize: 38,
    metaLinks: () => [],
    note: '',
    readmeContent: '',
    readmeExists: false,
    readmeLoading: false,
    readmeError: '',
    dependencies: () => [],
    dependenciesTitle: '',
    dependenciesMonoName: false,
    versions: () => [],
    version: null,
    versionFallback: '',
    bindingLabel: '',
    bindingValue: '',
    releaseTag: '',
    publishedAt: null,
    prerelease: false,
    releaseNotes: '',
    logEmptyText: '',
    assets: () => [],
    assetValue: null,
    fileLabel: '',
    fileHint: '',
    noAssetText: '',
    primaryAction: 'install',
    actionDisabled: false,
    disabledLabel: '',
    installing: false,
    refreshing: false,
    gameBlocked: false,
  },
)

const emit = defineEmits<{
  /** 版本下拉变化。 */
  'update:version': [value: string]
  /** 资产下拉变化。 */
  'update:asset': [value: string]
  /** 头部链接被点击（由调用方决定怎么打开）。 */
  'open-link': [url: string]
  /** 主按钮：安装当前选中版本（无 tag 时由调用方回退到最新）。 */
  install: [tag?: string, asset?: string]
  /** 主按钮：刷新（没有缓存 Release 时）。 */
  refresh: []
}>()

/** 说明 / 下载安装；切换由面板自己持有，调用方换条目时用 `:key` 重置。 */
const activeTab = ref<'info' | 'install'>('info')

const sourceUrl = computed(() =>
  props.provider === 'gitee'
    ? `https://gitee.com/${props.owner}/${props.repository}`
    : `https://github.com/${props.owner}/${props.repository}`,
)

/* ===== 说明 tab ===== */
const noteHtml = computed(() => renderReadme(props.note))
const readmeHtml = computed(() => renderReadme(props.readmeContent))
/** README 区域是否出现：加载中、请求失败、有内容时都显示。 */
const readmeSectionVisible = computed(() => {
  if (props.readmeLoading) return true
  if (props.readmeError) return true
  return !!props.readmeExists
})
/** 备注为空时的兜底文案；`noteEmptyText` 传空串表示「什么都不显示」。 */
const noteEmptyVisible = computed(() => props.noteEmptyText !== '')
const emptyNoteText = computed(() => props.noteEmptyText || t('tools.thisRepositoryHasNoNote'))

/* ===== 下载安装 tab ===== */
const versionText = computed(() => props.versionFallback || props.version || '—')
const bindingText = computed(() => props.bindingLabel || t('tools.versionBinding'))
const publishedDay = computed(() => (props.publishedAt ? props.publishedAt.slice(0, 10) : ''))
const logHtml = computed(() => renderReadme(props.releaseNotes.trim()))
const hasLog = computed(() => !!props.releaseNotes.trim())
const emptyLogText = computed(() => props.logEmptyText || t('modsLocal.thisReleaseHasNoNotes'))

/** 版本下拉与资产下拉的 Naive 选项（外部传入的是纯数据，`SelectMixedOption` 要求 `type`，这里统一转换）。 */
const versionOptions = computed<SelectOption[]>(() =>
  props.versions.map((item) => ({ label: item.label, value: item.value })),
)
const assetOptions = computed<SelectOption[]>(() =>
  props.assets.map((asset) => ({ label: asset.name, value: asset.name })),
)

/** 当前选中的资产（下拉模式）：找不到就回落到第一个。 */
const selectedAsset = computed<ReleaseDetailAsset | null>(
  () => props.assets.find((asset) => asset.name === props.assetValue) ?? props.assets[0] ?? null,
)
const emptyAssetText = computed(
  () => props.noAssetText || t('tools.noAssetInThisReleaseMatchesTheAssetRules'),
)
const fileHintText = computed(
  () => props.fileHint || t('tools.repositoryFileModeDownloadsThisFileDirectly'),
)

const actionIsRefresh = computed(() => props.primaryAction === 'refresh')
const actionLoading = computed(() => (actionIsRefresh.value ? props.refreshing : props.installing))
const actionDisabled = computed(() => {
  if (props.gameBlocked) return true
  if (props.actionDisabled) return true
  if (actionIsRefresh.value) return false
  // 没有任何可安装资产 → 安装必然失败；仓库文件模式有 `fileLabel`，不受影响
  if (!props.fileLabel && !props.assets.length) return true
  return false
})
/** 安装时下发给调用方的 tag；仓库文件模式没有版本概念。 */
const installTag = computed(() => (props.fileLabel ? undefined : (props.version ?? undefined)))
/**
 * 安装时下发的资产名。
 *
 * 必须和下拉里当前显示的**是同一个**文件：后端拿它精确匹配资产，只有
 * 传下去才能保证「界面上显示的」= 「实际下载的」。仓库文件模式没有资产。
 */
const installAsset = computed(() => (props.fileLabel ? undefined : (selectedAsset.value?.name ?? undefined)))

function runPrimaryAction() {
  if (actionIsRefresh.value) emit('refresh')
  else emit('install', installTag.value, installAsset.value)
}

/* ===== Markdown / Mermaid ===== */
const noteElement = ref<HTMLElement | null>(null)
const logElement = ref<HTMLElement | null>(null)
const readmeElement = ref<HTMLElement | null>(null)
watch(
  [noteHtml, logHtml, readmeElement, () => props.readmeLoading],
  async () => {
    await nextTick()
    void renderMermaidBlocks(noteElement.value)
    void renderMermaidBlocks(logElement.value)
    void renderMermaidBlocks(readmeElement.value)
  },
  { flush: 'post' },
)
</script>

<template>
  <div class="detail">
    <header class="detail-head">
      <div class="head-main">
        <ProviderIcon :provider="provider" :owner="owner" :size="iconSize" />
        <div class="head-text">
          <b>{{ title }}</b>
          <!-- 渠道 · 用户/仓库 · Release/Raw：后两段可点 -->
          <small class="head-meta">
            <span class="meta-channel">{{ provider === 'gitee' ? 'Gitee' : 'GitHub' }}</span>
            <span class="meta-sep">·</span>
            <button type="button" class="meta-link" :title="sourceUrl" @click="emit('open-link', sourceUrl)">
              {{ owner }}/{{ repository }}
            </button>
            <template v-for="link in metaLinks" :key="link.label + link.url">
              <span class="meta-sep">·</span>
              <button type="button" class="meta-link" :title="link.url" @click="emit('open-link', link.url)">
                {{ link.label }}
              </button>
            </template>
          </small>
        </div>
      </div>
      <!-- 状态标签（地图用它显示 最新版 / 有更新 / 依赖缺失）；MOD 不传则为空 -->
      <slot name="status" />
    </header>

    <!-- 分段切换：说明（依赖表 / 备注 / README）/ 下载安装（版本 / 日志 / 资产 / 安装） -->
    <n-tabs v-model:value="activeTab" type="segment" animated class="detail-tabs">
      <n-tab-pane name="info" :tab="t('tools.details')">
        <!--
          依赖表固定在备注**上方**并用分割线隔开：先回答「装上要什么」，
          再讲「这是什么」。依赖为空时整块（含分割线）不渲染。
        -->
        <template v-if="dependencies.length">
          <DependencyTable :rows="dependencies" :title="dependenciesTitle || t('tools.dependency')"
            :mono-name="dependenciesMonoName" />
          <n-divider />
        </template>

        <section v-if="note" ref="noteElement" class="note markdown-body" v-html="noteHtml"></section>
        <section v-else-if="noteEmptyVisible" class="note muted">{{ emptyNoteText }}</section>

        <!-- README：加载中显示骨架动画；文件不存在时整块（含分割线）收起 -->
        <Transition name="readme-collapse">
          <div v-if="readmeSectionVisible" class="readme-section">
            <n-divider />

            <section class="readme">
              <Transition name="readme-content" mode="out-in">
                <div v-if="readmeLoading" key="loading" class="readme-loading" role="status"
                  :aria-label="t('tools.loadingRepositoryReadme')">
                  <!-- 宽度与圆角统一在 .readme-loading :deep(.n-skeleton) 里定义 -->
                  <n-skeleton text class="skeleton-line" />
                  <n-skeleton text class="skeleton-line" />
                  <n-skeleton text class="skeleton-line" />
                  <n-skeleton text class="skeleton-line" />
                </div>
                <div v-else-if="readmeError" key="error" class="muted">{{ readmeError }}</div>
                <div v-else key="body" ref="readmeElement" class="markdown-body" v-html="readmeHtml"></div>
              </Transition>
            </section>
          </div>
        </Transition>
      </n-tab-pane>

      <!-- 面板 2：下载安装（版本选择 / 提交日志 / 资产与安装） -->
      <n-tab-pane name="install" :tab="t('tools.download')">
        <!-- 指标 + 安装合并区：版本下拉 / 版本绑定 / 资产信息 / 安装 -->
        <div class="summary">
          <div class="summary-row">
            <div class="metric metric-version">
              <span class="muted">{{ t('tools.selectVersion') }}</span>
              <n-select v-if="versions.length" :value="version" class="version-select" size="small"
                :options="versionOptions" :placeholder="t('tools.selectVersion')"
                @update:value="(value: string) => emit('update:version', value)" />
              <b v-else class="mono">{{ versionText }}</b>
            </div>
            <div v-if="bindingValue" class="metric metric-id">
              <span class="muted">{{ bindingText }}</span>
              <b class="mono">{{ bindingValue }}</b>
            </div>
          </div>

          <!-- 第二段：当前版本的提交日志（独立滚动，最大高度固定） -->
          <div class="release-log">
            <div class="log-head">
              <span class="muted">{{ t('modsLocal.releaseNotes') }}</span>
              <!-- 优先显示 Release tag（原始写法），name 为空时也用它 -->
              <b class="mono">{{ releaseTag || '—' }}</b>
              <span v-if="publishedDay" class="muted">{{ publishedDay }}</span>
              <!-- 预发布：元数据标记 或 版本号带 - 后缀 -->
              <n-tag v-if="prerelease" size="tiny" type="warning" :bordered="false">{{ t('modsOnline.preRelease') }}</n-tag>
            </div>
            <div v-if="hasLog" ref="logElement" class="log-box markdown-body" v-html="logHtml"></div>
            <div v-else class="log-box log-empty muted">{{ emptyLogText }}</div>
          </div>

          <!-- 资产选择 / 大小与下载次数 / 安装按钮 -->
          <div class="install-row">
            <!-- 左侧：可选已识别的 Release 资产（或仓库文件模式的文件） -->
            <n-select v-if="assets.length" class="asset-select" size="small"
              :value="selectedAsset?.name ?? null"
              :options="assetOptions"
              :placeholder="t('tools.selectFile')"
              @update:value="(value: string) => emit('update:asset', value)" />
            <span v-else-if="fileLabel" class="asset-select-file mono">{{ fileLabel }}</span>
            <span v-else class="asset-select-file muted">{{ emptyAssetText }}</span>

            <!-- 中右：大小与下载次数 -->
            <div class="asset-stats">
              <template v-if="fileLabel">
                <span class="muted">{{ fileHintText }}</span>
              </template>
              <template v-else-if="selectedAsset">
                <span class="stat-item">
                  <span class="muted">{{ t('tools.size') }}</span>
                  <b>{{ formatFileSize(selectedAsset.size) }}</b>
                </span>
                <span class="stat-item">
                  <span class="muted">{{ t('dialogs.downloads') }}</span>
                  <b>{{ (selectedAsset.download_count ?? 0).toLocaleString() }}</b>
                </span>
              </template>
            </div>

            <!-- 最右：安装 / 刷新 -->
            <n-button type="primary" :loading="actionLoading" :disabled="actionDisabled" @click="runPrimaryAction">
              <DownloadOutline v-if="!actionIsRefresh" width="15" height="15" />{{ actionIsRefresh
                ? t('modsOnline.refresh')
                : t('modsOnline.install') }}
            </n-button>
          </div>

          <!-- 仓库文件模式的下载方式已由安装行表达，这里只保留未启用提示 -->
          <div v-if="disabledLabel" class="summary-tags">
            <n-tag size="small" :bordered="false">{{ disabledLabel }}</n-tag>
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
.markdown-body {
  font-size: 12.5px;
  line-height: 1.7;
  color: var(--oc2-text-2);
  overflow-wrap: anywhere;
  /* Markdown 输出的宽表格/长代码行不撑破抽屉，超出时横向滚动 */
  overflow-x: auto;
}
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
@media (prefers-reduced-motion: reduce) {
  .readme-content-enter-active,
  .readme-content-leave-active { transition: none; }
}
@media (prefers-reduced-motion: reduce) {
  .readme-collapse-enter-active,
  .readme-collapse-leave-active { transition: none; }
}
</style>
