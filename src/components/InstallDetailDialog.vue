<script setup lang="ts">
/**
 * 「安装详情」弹窗 —— MOD 安装与地图导入共用的唯一展示组件。
 *
 * 只做三件事：把 `InstallDetailModel` 渲染成可展开的条目卡片、把底栏两个
 * 按钮的意图发出去、把离场动画结束的时刻交给宿主。它不认识 `ModPreview`，
 * 也不认识地图 ZIP —— 「一个 DLL 显示哪九项字段」「一个地图集怎么排版」
 * 全在 `installDetail.ts` 里算，宿主只把算好的模型递进来。
 *
 * 这样两个域不再各写一套卡片：MOD 与地图的差别只在数据来源（DLL 清单 /
 * `info` 解析结果），展示与交互完全一致。
 */
import { computed, nextTick, watch } from 'vue'
import { NButton, NCollapseTransition, NTag } from 'naive-ui'
import DependencyTable from './DependencyTable.vue'
import { tagColorFor } from '../theme'
import { formatVersion } from '../modStatus'
import { t } from '../i18n'
import AppModal from './AppModal.vue'
import LocalItemCard from './LocalItemCard.vue'
import { useCardExpansion } from '../composables/useCardExpansion'
import { collapseCardWithScroll, scrollExpandedCardIntoView } from '../expandScroll'
import type { InstallDetailEntry, InstallDetailModel } from '../installDetail'

const props = defineProps<{
  show: boolean
  /** 待确认的内容；null 表示还没有可展示的东西（弹窗内容区留空）。 */
  model: InstallDetailModel | null
  /** 正在安装：确认按钮转圈并禁用，避免重复提交。 */
  busy?: boolean
  /**
   * 取消按钮是否禁用。
   *
   * 与 `busy` 分开是因为「等用户表态」和「正在写盘」是两件事：地图导入的整个
   * 流程（含等待确认）都是 busy，但那时候取消恰恰必须能点；只有真正开始写盘
   * 之后才该拦住取消。
   */
  cancelDisabled?: boolean
}>()
const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  /** 用户点「立即安装」。 */
  (event: 'confirm'): void
  /** 用户放弃（取消按钮、叉掉、点遮罩、Esc）。 */
  (event: 'discard'): void
  /** 离场动画结束：宿主要等到这时才能清空数据，否则内容先消失只剩空框。 */
  (event: 'afterLeave'): void
}>()

/**
 * 渲染行：只列顶级条目（地图集 / MOD）。
 *
 * 随包插件与依赖表不占列表行 —— 它们挂在所属条目的 `children` /
 * `dependencies` 上，只有点开那张卡才出现在展开区里。这样收起态永远是一张卡
 * 一行：用户先确认「要装哪几张图」，再逐张看它会带进哪些 DLL、依赖够不够，
 * 与地图管理页详情的「信息 → 随包插件 → 依赖表」同一顺序。
 */
interface DetailRow {
  entry: InstallDetailEntry
  key: string
}

const rows = computed<DetailRow[]>(() =>
  (props.model?.entries ?? []).map((entry) => ({ entry, key: entry.key })))

/**
 * 条目展开状态：两级卡片共用 `useCardExpansion`（与 MOD 管理页、地图管理页同一份实现）。
 *
 * 外层是地图集 / MOD，内层是它的随包插件。硬规则与管理页一致：外层收起时连带
 * 清掉它的内层，下次展开是一张全新的、没点开过的卡片。
 */
const expansion = useCardExpansion()
const { isOuterOpen, isInnerOpen } = expansion

/** 展开区的内容层：展开滚动要按它的自然高度算「卡片最终会有多高」。 */
function detailBodyOf(card: Element) {
  return card.querySelector('.detail-body')
}

/**
 * 点开 / 收起一级条目。
 *
 * 滚动交给 `expandScroll`（与管理页同一套规则）：展开时把卡片底边推到视口可用
 * 底边、放不下才退化成贴顶；收起时把折叠后的卡片滚到可视区正中。弹窗内容区是
 * `.app-modal-scroll`，`resolveScrollContainer` 会自己找到它，这里不传容器。
 */
function onRowActivate(key: string, card: HTMLElement) {
  if (isOuterOpen(key)) {
    collapseCardWithScroll({ card, body: detailBodyOf(card) }, () => expansion.closeOuter(key))
    return
  }
  expansion.openOuter(key)
  /*
   * 内容层要等这一帧渲染完才拿得到：`n-collapse-transition` 在收起状态下
   * 不渲染插槽（折叠时子节点是注释占位），点开的那一刻 `querySelector` 还是空。
   */
  void nextTick(() => {
    scrollExpandedCardIntoView({
      card,
      anchor: card.querySelector('.detail-row-line') ?? card,
      body: detailBodyOf(card),
    })
  })
}

/** 点开 / 收起随包插件：外壳复用 `LocalItemCard`，滚动同一条规则。 */
function onNestedActivate(parentKey: string, childKey: string, card: HTMLElement) {
  if (isInnerOpen(parentKey, childKey)) {
    collapseCardWithScroll({ card, body: card.querySelector('.card-detail') }, () => expansion.closeInner(parentKey, childKey))
    return
  }
  expansion.openInner(parentKey, childKey)
  void nextTick(() => {
    scrollExpandedCardIntoView({
      card,
      anchor: card.querySelector('.card-row') ?? card,
      body: card.querySelector('.card-detail'),
    })
  })
}

/** 每次打开都从全部收起开始：上一份包展开过哪几条，对下一份没有意义。 */
watch(() => props.show, (open) => { if (open) expansion.closeAll() })
</script>

<template>
  <!--
    统一弹窗外壳：尺寸不超过可视区域，中间内容滚动，底部操作区固定。
    展开任意多条目、提交日志再长，弹窗大小与按钮位置都不会变。
  -->
  <AppModal :show="show" :title="model?.title ?? t('tools.installDetails')" card-class="install-dialog"
    @update:show="(value: boolean) => emit('update:show', value)"
    @after-leave="emit('afterLeave')">
    <div v-if="model" class="install-detail">
      <!-- 头部：包名 + 类型 + 一行计数摘要 -->
      <div class="detail-head">
        <b>{{ model.head.name }}</b>
        <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">
          {{ model.head.kind }}
        </n-tag>
        <span class="muted">{{ model.head.summary }}</span>
      </div>

      <!--
        冲突警示：目标位置已有同身份的条目，安装前会先把它们卸载（进回收站）。
        放在头部与条目列表之间 —— 用户第一眼该看到「这次安装会先移走什么」，
        再往下看这次要装进什么；同版本与否只影响行尾标签，动作完全一样。
      -->
      <div v-if="model.conflicts.length" class="conflict-block" role="alert">
        <div class="conflict-head">{{ model.conflictHint }}</div>
        <div v-for="conflict in model.conflicts" :key="conflict.key" class="conflict-row">
          <span class="conflict-name" :title="conflict.name">「{{ conflict.name }}」</span>
          <span class="conflict-version mono">{{ conflict.version }}</span>
          <n-tag v-if="conflict.sameVersion" size="tiny" :bordered="false" :color="tagColorFor('muted')">
            {{ t('tools.installConflictSameVersion') }}
          </n-tag>
        </div>
      </div>

      <!--
        条目区：一级条目（地图集 / MOD）各占一张横排紧凑卡片。
        收起态是「名称 + 淡灰副标题 …… 角色标签 + 右侧版本」；点开才看到
        九项元数据、随包插件与依赖表 —— 与地图管理页详情的层级完全一致。
      -->
      <div v-if="rows.length" class="detail-list">
        <article v-for="row in rows" :key="row.key" class="detail-row"
          :class="{ expanded: isOuterOpen(row.key) }"
          @click="onRowActivate(row.key, $event.currentTarget as HTMLElement)">
          <div class="detail-row-line">
            <span class="detail-main">
              <b class="detail-name" :title="row.entry.title">{{ row.entry.title }}</b>
              <span v-if="row.entry.subtitle" class="detail-sub mono" :title="row.entry.subtitle">
                {{ row.entry.subtitle }}
              </span>
            </span>
            <span class="detail-side">
              <n-tag v-if="row.entry.tag" size="tiny" :bordered="false" :color="tagColorFor('muted')">
                {{ row.entry.tag }}
              </n-tag>
              <span class="detail-meta mono">{{ row.entry.meta ?? '' }}</span>
            </span>
          </div>
          <n-collapse-transition :show="isOuterOpen(row.key)" display-directive="show">
            <!-- 内容层单独包一层：展开滚动按它的自然高度算卡片最终高度 -->
            <div class="detail-body" @click.stop>
              <div class="detail-row-fields">
                <div v-for="(field, index) in row.entry.fields" :key="index" class="detail-item"
                  :class="{ wide: field.wide }">
                  <span class="muted">{{ field.label }}</span>
                  <b :class="{ mono: field.mono }">{{ field.value }}</b>
                </div>
              </div>
              <!--
                随包插件：外壳直接复用管理页的 LocalItemCard —— 标题后的版本号、
                灰色「随包插件」标签、点开看 DLL 元数据，与管理页同一套版式。
                它们跟着这张图落盘、跟着这张图启停，所以缩进在所属条目里。
              -->
              <div v-if="row.entry.children?.length" class="detail-plugins">
                <LocalItemCard v-for="child in row.entry.children" :key="child.key"
                  :switchable="false" :title="child.title" :version="child.meta"
                  :titleTag="child.tag" :expanded="isInnerOpen(row.key, child.key)"
                  @activate="(card: HTMLElement) => onNestedActivate(row.key, child.key, card)">
                  <template #detail>
                    <div class="detail-row-fields detail-row-fields--nested">
                      <div v-for="(field, index) in child.fields" :key="index" class="detail-item"
                        :class="{ wide: field.wide }">
                        <span class="muted">{{ field.label }}</span>
                        <b :class="{ mono: field.mono }">{{ field.value }}</b>
                      </div>
                    </div>
                  </template>
                </LocalItemCard>
              </div>
              <!-- 依赖表：与地图管理页同一份组件与判定；弹窗里不提供跳转 -->
              <div v-if="row.entry.dependencies?.length" class="detail-deps">
                <DependencyTable :rows="row.entry.dependencies" />
              </div>
            </div>
          </n-collapse-transition>
        </article>

      </div>
      <!-- 空态：没有可安装的内容时说明「为什么」以及「该去哪里」 -->
      <div v-else-if="model.emptyHint.length" class="empty-hint">
        <div v-for="line in model.emptyHint" :key="line" class="muted">{{ line }}</div>
      </div>

      <!--
        仓库来源才展示本次 Release 的提交日志：用户点「立即安装」之前
        应该能看到这次到底更新了什么。本地文件与直链没有版本概念，不显示。
      -->
      <!-- model.releaseLog 已保证正文非空，这里不需要「没有日志」的占位分支 -->
      <div v-if="model.releaseLog" class="release-log">
        <div class="release-log-head">
          <span class="muted">{{ t('modsLocal.releaseNotes') }}</span>
          <b class="mono">{{ formatVersion(model.releaseLog.tag) }}</b>
        </div>
        <!-- renderReadme 关闭了原始 HTML，日志只能输出 Markdown 生成的安全节点 -->
        <div class="release-log-box" v-html="model.releaseLog.html"></div>
      </div>
    </div>

    <template #footer>
      <n-button secondary :disabled="cancelDisabled" @click="emit('discard')">
        {{ model?.cancelLabel ?? t('recycle.cancel') }}
      </n-button>
      <n-button type="primary" :loading="busy" :disabled="!model?.confirmable" @click="emit('confirm')">
        {{ model?.confirmLabel ?? t('tools.installNow') }}
      </n-button>
    </template>
  </AppModal>
</template>

<style scoped>
.install-detail {
  display: flex;
  flex-direction: column;
  gap: 12px;
  /*
   * 整条 flex 链一律不收缩：内容比弹窗高时，压缩会把卡片内部网格压扁、
   * 后面的随包插件与依赖表按压扁后的位置叠上来（实测用户截图里的错位）。
   * 高度超出就交给 `.app-modal-scroll` 滚，卡片自身永远保持完整版式。
   */
  flex: 0 0 auto;
}
.detail-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.detail-head b { font-size: 13.5px; color: var(--oc2-text-1); }
/*
 * 条目区不自己限高：展开详情本该完整可见，一旦内部再滚动，
 * 卡片下半截会被裁在容器里，看起来像渲染坏了。
 * 整体高度由弹窗内容区承担（见 AppModal 的 .app-modal-scroll）。
 */
.detail-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 0 0 auto;
}
/* 空态两行：一句话说清「没有内容」，一句话说清「所以装不了、地图包去哪」 */
.empty-hint {
  display: grid;
  gap: 4px;
  padding: 10px 4px;
  line-height: 1.6;
}
/*
 * 条目卡片：与 MOD 管理页的 .mod-card 同一套紧凑横排语言 ——
 * 圆角外边框、单行「名称居左 / 版本居右」，展开后追加详情网格。
 * 这里不放置开关、状态标签与操作按钮：安装前只需要确认内容。
 */
.detail-row {
  display: flex;
  flex-direction: column;
  padding: 9px 12px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  background: var(--oc2-card);
  cursor: pointer;
  flex: 0 0 auto;
  transition: border-color .16s ease, background-color .16s ease;
}
.detail-row:hover { border-color: var(--oc2-accent); background: var(--oc2-row-hover); }
.detail-row.expanded { border-color: var(--oc2-accent); }
/*
 * 展开区里的两个从属区块：随包插件、依赖表。
 * 各用一条分隔线起头，和上面九项元数据网格同一层级语言；插件缩进成更深一层
 * 的卡片（它是这张图的附属物），依赖表是纯表格、不再套卡片。
 */
.detail-plugins {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--oc2-row-line);
}
/*
 * 随包插件卡片复用 LocalItemCard，底色走它的 `--card-bg`：
 * 与管理页 `.package-body` 里的子卡片同样用下沉底色，两处一眼是同一个层级。
 */
.detail-plugins .mod-card {
  --card-bg: var(--oc2-sunken);
}
/* 嵌套卡片的详情区不需要再套一条分隔线：卡片外框已经把它分开 */
.detail-row-fields--nested {
  margin-top: 0;
  padding-top: 0;
  border-top: none;
}
.detail-deps {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--oc2-row-line);
}
/*
 * 右侧收尾列：角色标签 + 版本号。
 *
 * 两段贴在一起（8px），与左侧标题区之间由 .detail-row-line 的 14px gap 分开，
 * 扫视时「这是随包插件 / 版本 2.2.6」是一组，不会被读成两个独立字段。
 */
.detail-side {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 0 0 auto;
}
.detail-row-line {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 14px;
  min-width: 0;
}
/*
 * 左侧「名称 + 副标题」自己是一组：副标题允许被压缩并省略，
 * 右侧 meta 列 flex: 0 0 auto 永不缩，所以副标题只会避让、不会顶开它。
 */
.detail-main {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
  flex: 1 1 auto;
}
.detail-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--oc2-text-1);
  white-space: nowrap;
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.detail-sub {
  font-size: 11.5px;
  color: var(--oc2-text-4);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  /* 副标题是次要信息：优先让名称保持完整 */
  flex: 1 1 auto;
  min-width: 0;
}
.detail-meta { font-size: 12px; color: var(--oc2-text-3); flex: 0 0 auto; }
/* 展开后的元数据网格：与 MOD 管理页详情区同款两列布局 */
.detail-row-fields {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 16px;
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--oc2-row-line);
}
.detail-item { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.detail-item.wide { grid-column: 1 / -1; }
.detail-item span { font-size: 11px; }
.detail-item b {
  font-size: 12px;
  font-weight: 500;
  color: var(--oc2-text-1);
  overflow-wrap: anywhere;
}
/* 冲突警示块：语义色走主题变量，换明暗/主色时跟着变 */
.conflict-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 9px 12px;
  border: 1px solid color-mix(in srgb, var(--oc2-tone-moderate-bg, #f59e0b) 32%, transparent);
  border-radius: 10px;
  background: color-mix(in srgb, var(--oc2-tone-moderate-bg, #f59e0b) 10%, transparent);
}
.conflict-head {
  font-size: 12px;
  line-height: 1.6;
  color: var(--oc2-text-2);
}
.conflict-row {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
}
.conflict-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--oc2-text-1);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.conflict-version { font-size: 12px; color: var(--oc2-text-3); }
/* Release 提交日志：与仓库详情抽屉的日志区块保持同一视觉语言 */
.release-log { display: flex; flex-direction: column; gap: 6px; }
.release-log-head {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}
.release-log-head span { font-size: 11px; }
.release-log-head b {
  font-size: 12px;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/*
 * 日志盒子不再自己限高滚动。
 *
 * 外层的 .app-modal-scroll 已经负责滚动；这里再套一层内滚动会出现
 * 两级滚动：用户在日志框里滚到底，还得把指针移出来再滚弹窗，
 * 而中间的条目区往往只有一两行，体验很别扭。
 * 现在日志按内容自然铺开，整段由弹窗统一滚动，滚动手势只有一级。
 */
.release-log-box {
  padding: 8px 10px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 8px;
  background: var(--oc2-sunken);
  font-size: 12px;
  line-height: 1.65;
  /* Markdown 输出的宽表格/长代码行不撑破弹窗，超出时横向滚动 */
  overflow-x: auto;
}
/*
 * v-html 插入的 Markdown 节点不会带 scoped 属性，
 * 所以这些规则必须用 :deep 才能作用到 renderReadme 的输出上。
 */
.release-log-box :deep(p) { margin: 0 0 8px; }
.release-log-box :deep(> :last-child) { margin-bottom: 0; }
.release-log-box :deep(h1),
.release-log-box :deep(h2),
.release-log-box :deep(h3),
.release-log-box :deep(h4) {
  margin: 12px 0 6px;
  font-size: 13px;
  font-weight: 600;
  color: var(--oc2-text-1);
}
.release-log-box :deep(> :first-child) { margin-top: 0; }
.release-log-box :deep(ul),
.release-log-box :deep(ol) { margin: 0 0 8px; padding-left: 20px; }
.release-log-box :deep(li) { margin: 2px 0; }
.release-log-box :deep(code) {
  padding: 1px 5px;
  border-radius: 4px;
  background: var(--oc2-row-hover);
  font-size: 11.5px;
}
.release-log-box :deep(pre) {
  margin: 0 0 8px;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--oc2-row-hover);
  overflow-x: auto;
}
.release-log-box :deep(pre code) { padding: 0; background: none; }
.release-log-box :deep(a) { color: var(--oc2-accent); text-decoration: none; }
.release-log-box :deep(a:hover) { text-decoration: underline; }
.release-log-box :deep(img) { max-width: 100%; border-radius: 6px; }
</style>
