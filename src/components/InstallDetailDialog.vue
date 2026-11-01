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
import { computed, ref, watch } from 'vue'
import { NButton, NCollapseTransition, NTag } from 'naive-ui'
import { tagColorFor } from '../theme'
import { formatVersion } from '../modStatus'
import { t } from '../i18n'
import AppModal from './AppModal.vue'
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
 * 渲染行：先全部顶级条目，再全部下级条目（随包插件）。
 *
 * 下级条目不缩进、也不跟在所属条目后面，而是整批排到列表最底 —— 缩进只能
 * 表达「从属」，代价是右侧那列被挤窄、多张图各有插件时缩进层级还会叠起来；
 * 改成平铺 + 右侧「随包插件」标签后，归属靠标签说明，版本号也回到自己该在
 * 的位置。key 带上父条目，避免两张图的插件包内路径相同时撞 key。
 *
 * 用扁平数组渲染而不是递归组件：下级条目只有一个层级，卡片本身的样式
 * 与交互和顶级完全一样。
 */
interface DetailRow {
  entry: InstallDetailEntry
  key: string
}

const rows = computed<DetailRow[]>(() => {
  const top: DetailRow[] = []
  const children: DetailRow[] = []
  for (const entry of props.model?.entries ?? []) {
    top.push({ entry, key: entry.key })
    for (const child of entry.children ?? []) {
      children.push({ entry: child, key: `${entry.key}\u0000${child.key}` })
    }
  }
  return [...top, ...children]
})

/**
 * 条目展开状态：默认全部收起，只记展开的 key。
 *
 * 收起态是「名称 …… 版本」一行，用户不关心细节时整块只有几行高，
 * 提交日志因此能拿到更大的可视范围。
 */
const expandedKeys = ref<string[]>([])
function isExpanded(key: string) {
  return expandedKeys.value.includes(key)
}
function toggle(key: string) {
  expandedKeys.value = isExpanded(key)
    ? expandedKeys.value.filter((item) => item !== key)
    : [...expandedKeys.value, key]
}
/** 每次打开都从全部收起开始：上一份包展开过哪几条，对下一份没有意义。 */
watch(() => props.show, (open) => { if (open) expandedKeys.value = [] })
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
        条目区：全都是同一张横排紧凑卡片。
        收起态是「名称 + 淡灰副标题 …… 角色标签 + 右侧版本」；点击展开完整元数据。
        随包插件平铺在列表最底，靠右侧标签而不是缩进说明归属。
      -->
      <div v-if="rows.length" class="detail-list">
        <article v-for="row in rows" :key="row.key" class="detail-row"
          :class="{ expanded: isExpanded(row.key) }"
          @click="toggle(row.key)">
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
          <n-collapse-transition :show="isExpanded(row.key)" display-directive="show">
            <div class="detail-row-fields">
              <div v-for="(field, index) in row.entry.fields" :key="index" class="detail-item"
                :class="{ wide: field.wide }">
                <span class="muted">{{ field.label }}</span>
                <b :class="{ mono: field.mono }">{{ field.value }}</b>
              </div>
            </div>
            <div v-if="row.entry.note" class="detail-note">{{ row.entry.note }}</div>
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
  transition: border-color .16s ease, background-color .16s ease;
}
.detail-row:hover { border-color: var(--oc2-accent); background: var(--oc2-row-hover); }
.detail-row.expanded { border-color: var(--oc2-accent); }
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
/* 卡片底部提示（例如「已有同名地图集，旧版本先进回收站」）：附在详情网格下面 */
.detail-note {
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px dashed var(--oc2-row-line);
  font-size: 11.5px;
  line-height: 1.6;
  color: var(--oc2-text-3);
}
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
