<script setup lang="ts">
/**
 * 在线内容卡片（MOD 仓库 / 地图仓库共用）。
 *
 * ## 为什么下沉成组件
 *
 * 两个页面的卡片外观完全一致：图标 + 标题 + 状态标签、两段式版本号、下载量、
 * 主操作按钮，连 CSS 都是同一套 `--oc2-*` 变量。差异只在**数据从哪来**：
 *
 * - MOD 卡片的状态要对照已装插件的 GUID 与版本；
 * - 地图卡片的状态要对照本地关卡集的 `author + levelSetName + levelSetNameZH`。
 *
 * 所以这里只做**展示**：所有状态、动作文案、按钮配色都由调用方算好后传入。
 * 组件不碰 `ModRepository` / `MapInfo` 任何一种模型 —— 一旦在这里 `instanceof`
 * 判断类型，两边就会开始互相牵连。
 *
 * ## 图标必须放 `#icon` 槽
 *
 * Naive 只用 `IconSwitchTransition` 切换这个槽位（图标 ↔ spinner）。放默认
 * 插槽里会被渲染两份：既进图标槽、又作为文字前缀再渲染一次，loading 时就是
 * 「spinner + SVG 并存」。
 */
import { NButton, NTag } from 'naive-ui'
import { ArrowUpCircleOutline, DownloadOutline, RefreshOutline } from '@vicons/ionicons5'
import { buttonColorFor, tagColorFor, tagColorForAny, type TagTone } from '../theme'
import { t } from '../i18n'
import { translateBackendMessage } from '../locale/backend'
import type { VersionDisplay } from '../modStatus'
import ProviderIcon from './ProviderIcon.vue'
import type { Provider } from '../types'

/** 主按钮的动作类型，决定图标。 */
export type CardActionKind = 'install' | 'reinstall' | 'update' | 'refresh'

export interface CardAction {
  kind: CardActionKind
  label: string
  loading?: boolean
  disabled?: boolean
}

/** 标题左侧的关系提示（缺依赖 / 冲突）。 */
export interface CardDependencyTag {
  text: string
  tone: TagTone
  title?: string
  /** 有可跳转目标时才可点。 */
  clickable?: boolean
}

const props = defineProps<{
  provider: Provider
  owner?: string
  title: string
  /** 简介（调用方已做 markdown 摘要与长度处理）。 */
  note: string
  /** 状态标签文案与色调（由调用方按各自领域算出）。 */
  statusText: string
  statusTone: string
  /** 状态标签可点（例如跳到管理页）。 */
  statusClickable?: boolean
  statusTitle?: string
  dependency?: CardDependencyTag | null
  version: VersionDisplay
  /** 该行是否正在刷新（版本号旁转圈）。 */
  refreshing?: boolean
  downloads?: number | null
  action: CardAction
  /**
   * 该行刷新失败的**原始**错误：后端 tag（可带 `?k=v` 参数）或普通整句。
   *
   * 刷新引擎只负责把后端给的原样写回 `last_error`（它还兼任「这一行失败了」
   * 的判据），翻译在**这里**做一次 —— 卡片是两个在线页共用的展示层，漏在
   * 某个调用点就会把 `mapSource.noAssetInTag?map=…` 这种裸 tag 直接摆到界面上。
   */
  error?: string | null
  /** 详情抽屉选中态。 */
  active?: boolean
  /** 跨页/页内跳转高亮。 */
  focusFlash?: boolean
}>()

const emit = defineEmits<{
  (event: 'open'): void
  (event: 'status-click'): void
  (event: 'dependency-click'): void
  (event: 'action'): void
}>()

/**
 * 主按钮配色：动作 → 语义色调 + 视觉强度。
 *
 * 规则与标签共用同一个 tone 刻度（`theme.ts`），两边一致：
 *   安装 / 重装 → accent 实心（「装到最新」这一档，不需要警告）
 *   更新        → moderate 实心（需要用户注意的动作）
 *   刷新（失败重试）→ moderate 浅底（提示可以点这里，不该比旁边的标签更抢眼）
 */
function actionStyle(action: CardAction) {
  if (action.kind === 'refresh') return buttonColorFor('moderate', 'soft')
  if (action.kind === 'update') return buttonColorFor('moderate', 'solid')
  return buttonColorFor('accent', 'solid')
}

/** 依赖标签的悬停提示：没有自定义时给一句兜底，避免空 title。 */
function dependencyTitle() {
  return props.dependency?.title
}

function openCard() {
  emit('open')
}

function clickStatus() {
  if (props.statusClickable) emit('status-click')
}

function clickDependency() {
  if (props.dependency?.clickable) emit('dependency-click')
}

function runAction() {
  if (props.action.disabled || props.action.loading) return
  emit('action')
}
</script>

<template>
  <article class="online-card" :class="{ active, 'focus-flash': focusFlash }" @click="openCard">
    <div class="card-head">
      <ProviderIcon :provider="provider" :owner="owner" :size="30" />
      <!--
        关系提示：**只做提示，不做入口**。
        真正的处理由点主按钮时的安装前检查一次跑完，所以这里不放
        「去卸载」「安装依赖」按钮 —— 同一件事给两个入口，只会让人先去点
        标题前那个更显眼的按钮，结果装完依赖发现主包没装。

        标签本身可以点：跳到**依赖的**仓库并高亮它。没有可安装来源时不假装
        可点（跳过去也无处可去）。
      -->
      <n-tag v-if="dependency" size="tiny" :bordered="false" class="state-tag dep-tag"
        :class="{ 'state-tag--link': dependency.clickable }"
        :color="tagColorFor(dependency.tone)" :title="dependencyTitle()" @click.stop="clickDependency">
        {{ t(dependency.text) }}
      </n-tag>
      <span class="card-title">{{ title }}</span>
      <span class="stat stat-status">
        <n-tag size="tiny" :bordered="false" class="state-tag"
          :color="tagColorForAny(statusTone)" :class="{ 'state-tag--link': statusClickable }"
          :title="statusTitle" @click.stop="clickStatus">{{ statusText }}</n-tag>
      </span>
    </div>

    <p class="card-desc">{{ note }}</p>

    <div class="card-foot">
      <span class="stat stat-version">
        <span class="muted">{{ t('dialogs.latestVersion') }}</span>
        <span class="version-line">
          <!--
            两段式：主干用普通色，测试标记用 prerelease 等级色。
            数字序号渲染成灰底徽标（与分组标题的计数同一套外观），
            文字标记（如「测」）直接拼在后面。
          -->
          <b class="mono version-base">{{ version.base }}</b>
          <n-tag v-if="version.markerIsNumeric" size="tiny" :bordered="false" class="version-marker"
            :color="tagColorFor('moderate')">{{ version.marker }}</n-tag>
          <b v-else-if="version.marker" class="mono version-marker-text"
            :style="{ color: tagColorFor('moderate').textColor }">{{ version.marker }}</b>
          <!-- 刷新状态：转圈即表示该行正在拉取 Release -->
          <RefreshOutline v-if="refreshing" class="spin-icon" width="12" height="12" />
        </span>
      </span>
      <!--
        下载量：为 0 或没获取到整块隐藏，而不是显示「—」。
        一个永远显示破折号的统计项只会占地方，还让人怀疑数据坏了。
      -->
      <span v-if="downloads" class="stat card-downloads">
        <span class="muted">{{ t('dialogs.downloads') }}</span>
        <b>{{ downloads.toLocaleString() }}</b>
      </span>
      <!-- 图标放 #icon 槽，才会被 Naive 的 IconSwitchTransition 与 spinner 互换 -->
      <n-button size="tiny" class="card-install" :style="actionStyle(action)" :loading="action.loading"
        :disabled="action.disabled" @click.stop="runAction">
        <template #icon>
          <RefreshOutline v-if="action.kind === 'refresh' || action.kind === 'reinstall'" width="13"
            height="13" />
          <ArrowUpCircleOutline v-else-if="action.kind === 'update'" width="13" height="13" />
          <DownloadOutline v-else width="13" height="13" />
        </template>
        {{ action.label }}
      </n-button>
    </div>

    <div v-if="error" class="err muted">{{ translateBackendMessage(error) }}</div>
  </article>
</template>

<style scoped>
/*
 * 卡片尺寸、圆角、hover 与高亮全部沿用原来的在线卡片样式，
 * 两页共用一份，不会再各自漂移。
 */
.online-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 12px 14px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
  cursor: pointer;
  transition: border-color .16s ease, background-color .16s ease, box-shadow .16s ease;
}
.online-card:hover {
  border-color: var(--oc2-accent);
  background: var(--oc2-row-hover);
  box-shadow: 0 4px 16px rgba(0, 0, 0, .16);
}
.online-card.active {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 8%, var(--oc2-card));
}
/* 跨页跳转高亮：3 秒后自动褪去（时长由 crossLink.ts 控制） */
.online-card.focus-flash {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 16%, var(--oc2-card));
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent);
  /* 跳转落点确认：一次轮廓脉冲 */
  animation: online-card-focus-pulse 1200ms cubic-bezier(0.4, 0, 0.2, 1);
}
@keyframes online-card-focus-pulse {
  0%   { box-shadow: 0 0 0 0 color-mix(in srgb, var(--oc2-accent) 70%, transparent); }
  40%  { box-shadow: 0 0 0 5px color-mix(in srgb, var(--oc2-accent) 30%, transparent); }
  100% { box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent); }
}
@media (prefers-reduced-motion: reduce) {
  .online-card.focus-flash { animation: none; }
}
/* 可点标签的手型与悬停反馈由 global.css 的 .state-tag--link 统一提供 */

.card-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
/*
 * 关系提示标签：固定在标题左侧、不随标题压缩消失。
 * 标题自己 flex:1 + 省略号，所以空间不足时先压标题，标签保持完整。
 */
.card-head .dep-tag {
  flex: 0 0 auto;
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
  /*
   * 单行 flex：版本号 → 下载量 → 按钮，从左到右。
   *
   * 空间不足时先压 .card-downloads（唯一可收缩项），再裁掉它；
   * 按钮 `flex: 0 0 auto` 永远保持完整并贴右。
   */
  display: flex;
  align-items: flex-end;
  gap: 14px;
  padding-top: 8px;
  border-top: 1px solid var(--oc2-row-line);
  min-width: 0;
}
.stat {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 1px;
  flex: 0 0 auto;
  min-width: 0;
  width: auto;
  text-align: center;
}
/*
 * 版本号必须尽量完整显示：优先压缩下载量，不够时才压缩版本号。
 * 强制单行 + 省略号 —— 版本号是无空白长串，允许折行会被逐字断开。
 */
.stat-version {
  flex: 0 1 auto;
  min-width: 0;
  max-width: 180px;
  overflow: hidden;
  white-space: nowrap;
  word-break: normal;
  overflow-wrap: normal;
}
.stat-version b {
  display: block;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 下载量是「可牺牲」项：空间不足时先压窄它，压到 0 就整体隐藏。 */
.card-downloads {
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  /* 覆盖 global.css 的 break-all，否则中文标签会在任意字间断开 */
  white-space: nowrap;
  word-break: normal;
  overflow-wrap: normal;
}
.version-line {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  min-width: 0;
  max-width: 100%;
}
/* 主干刻意不做成 prerelease 色：先读「哪个版本」，再读「第几个测试版」。 */
.version-base {
  color: var(--oc2-text-1);
}
.version-marker {
  --n-height: 18px;
  font-variant-numeric: tabular-nums;
}
.version-marker-text {
  font-size: 11.5px;
  font-weight: 600;
}
.spin-icon {
  flex: 0 0 auto;
  color: var(--oc2-accent);
  animation: oc2-online-spin .8s linear infinite;
}
@keyframes oc2-online-spin { to { transform: rotate(360deg); } }
@media (prefers-reduced-motion: reduce) {
  .spin-icon { animation: none; }
}
.stat b {
  font-size: 12.5px;
  color: var(--oc2-text-1);
  max-width: 100%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/*
 * 状态标签靠右，与下方主按钮同列对齐；宽度跟随文字，不拉伸。
 * 外观来自 global.css 的 .state-tag，这里只负责容器定位。
 */
.card-head .stat-status {
  margin-left: auto;
  flex: 0 0 auto;
  width: auto;
  min-width: 0;
  align-items: center;
}
/*
 * 按钮不可压缩：卡片变窄时被牺牲的是 .card-downloads，
 * 按钮永远保持完整宽度并贴住右边缘。
 */
.card-install {
  flex: 0 0 auto;
  margin-left: auto;
  white-space: nowrap;
}
.err { color: #ee8585; }
</style>
