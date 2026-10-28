<script setup lang="ts">
/**
 * 「查看安装详情」弹窗 —— 所有安装入口共用的唯一预览界面。
 *
 * 职责边界（解耦后的分工）：
 *   本组件只做三件事：
 *     1. 展示调用方传入的 `ModPreview`
 *     2. 用户点「立即安装」时调 confirm_mod_install
 *     3. 用户取消/关闭时通知调用方放弃这份预览
 *   它**不负责**获取预览：本地选文件、直链下载、仓库资产下载都在调用方，
 *   因为那三种来源的下载方式（文件选择器 / 直连 / 多源竞速）差异很大，
 *   塞进弹窗会让这里变成三种流程的交叉分支。
 *
 * 调用方把预览通过 `preview` prop 传进来，弹窗只负责渲染与确认，
 * 不持有预览副本 —— 否则调用方与弹窗各存一份，只有一方被更新时
 * 弹窗会打开但内容为空。
 */
import { computed, ref, watch } from 'vue'
import { NButton, NCollapseTransition, NTag } from 'naive-ui'
import { api } from '../services/tauri'
import { tagColorFor } from '../theme'
import { renderReadme } from '../markdown'
import { formatVersion } from '../modStatus'
import { t } from '../i18n'
import { useNotify } from '../useNotify'
import { celebrateInstall } from '../celebration'
import AppModal from './AppModal.vue'
import type { ModPreview, ModPreviewEntry } from '../types'

const props = defineProps<{ show: boolean; preview: ModPreview | null }>()
const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  (event: 'installed'): void
  /** 用户放弃这份预览（取消按钮、叉掉），调用方据此清理暂存。 */
  (event: 'discarded', preview: ModPreview): void
}>()

const message = useNotify()
const installing = ref(false)

/**
 * 仓库来源才展示 Release 提交日志；本地文件与直链没有版本概念，整块不出现。
 * RAW 仓库虽然来源是仓库，但没有 Release，`release_tag` 为空，同样隐藏。
 */
const releaseLogHtml = computed(() =>
  props.preview?.origin === 'repository' ? renderReadme(props.preview.release_body ?? '') : '',
)
/**
 * 只有「仓库来源 + 有版本号 + 有日志正文」三者齐备时才展示提交日志区块。
 *
 * - 本地文件 / 直链：没有版本概念，整块不出现；
 * - RAW 仓库：没有 Release，`release_tag` 为空，整块不出现；
 * - 有版本但这次没写日志：也整块不出现 —— 显示一个空的「该版本没有提交日志」
 *   方框只是占地方，和仓库文件那两种情况一样应当直接省略。
 */
const showReleaseLog = computed(() =>
  props.preview?.origin === 'repository'
  && props.preview.release_tag.trim() !== ''
  && (props.preview.release_body ?? '').trim() !== '',
)

/**
 * 条目展开状态：默认全部收起，只记展开的 path。
 *
 * 收起态是「名称 …… 版本」一行，用户不关心细节时整块只有几行高，
 * 提交日志因此能拿到更大的可视范围。
 */
const expandedPaths = ref<string[]>([])
function isEntryExpanded(path: string) {
  return expandedPaths.value.includes(path)
}
function toggleEntry(path: string) {
  expandedPaths.value = isEntryExpanded(path)
    ? expandedPaths.value.filter((item) => item !== path)
    : [...expandedPaths.value, path]
}
/** 换一份预览时清空展开状态：上一份包的 path 对新包没有意义。 */
watch(() => props.preview?.preview_id, () => { expandedPaths.value = [] })

/**
 * 条目标题：先用 BepInPlugin 读到的插件名（用户在游戏里认识的名字），
 * 没有才退回文件名 —— 路径已经在旁边淡灰显示，不需要重复。
 */
function entryTitle(entry: ModPreviewEntry) {
  const name = entry.name?.trim()
  if (name) return name
  const file = entry.path.split('/').pop() ?? entry.path
  return file.replace(/\.dll$/i, '')
}

/**
 * 弹窗开关；关闭时先只收起，等离场动画播完再通知丢弃。
 *
 * 之前是「收起 + 立刻发 discarded」同时做，调用方马上把 preview 置空，
 * 弹窗内容先消失、只剩下一个空框在缩小，看不到渐隐。
 */
const visible = computed({
  get: () => props.show,
  set: (value: boolean) => {
    if (value) emit('update:show', true)
    else requestDiscard()
  },
})

/** 这次关闭是否由「用户放弃」触发（安装成功后不该再清理暂存）。 */
const discarding = ref(false)
/** 安装已成功、等离场动画结束后再通知宿主（否则内容先消失、看不到渐隐）。 */
const installCompleted = ref(false)

/** 用户放弃：立刻收起，动画结束后再通知调用方清理。 */
function requestDiscard() {
  discarding.value = true
  emit('update:show', false)
}

/**
 * 离场动画结束：此时内容已经淡出，可以安全清空并通知调用方。
 * Naive 的 modal 用 fade-in-scale-up-transition（缩小 + 渐隐）。
 */
function handleAfterLeave() {
  if (discarding.value) {
    discarding.value = false
    const current = props.preview
    if (current) emit('discarded', current)
  }
  if (installCompleted.value) {
    installCompleted.value = false
    emit('installed')
  }
}

/** 用户确认安装。 */
async function confirm() {
  const current = props.preview
  if (!current) return
  installing.value = true
  try {
    const result = await api.confirmModInstall(current.preview_id)
    if (!result.ok) {
      message.error(result.message)
      return
    }
    message.success(result.message)
    // 安装成功是低频、高价值时刻：放一次短促彩带（内部已尊重 reduced-motion）
    void celebrateInstall()
    // 后端已在 confirm 里删掉暂存；收起面板，动画结束后再通知宿主刷新
    installCompleted.value = true
    emit('update:show', false)
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    installing.value = false
  }
}
</script>

<template>
  <!--
    统一弹窗外壳：尺寸不超过可视区域，中间内容滚动，底部操作区固定。
    展开任意多条目、提交日志再长，弹窗大小与按钮位置都不会变。
  -->
  <AppModal :show="visible" :title="t('安装详情')" card-class="install-dialog"
    @update:show="(value: boolean) => (visible = value)"
    @after-leave="handleAfterLeave">
    <div v-if="preview" class="install-preview">
      <!-- 头部：包名 + 类型 + 三个计数 -->
      <div class="preview-head">
        <b>{{ preview.package_name }}</b>
        <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">
          {{ preview.kind === 'single' ? t('单个 MOD') : t('MOD 包') }}
        </n-tag>
        <span class="muted">
          {{ t('{count} 个 DLL · {mods} 个 MOD · {deps} 个依赖',
            { count: preview.total_dlls, mods: preview.mod_count, deps: preview.dependency_count }) }}
        </span>
      </div>

      <!--
        条目区：单 MOD 与 MOD 包统一成同一套横排紧凑卡片。
        收起态是「插件名 …… 版本」，插件名后紧跟淡灰的包内路径；
        路径自己截断省略，绝不挤压右侧版本列。
        点击展开该 DLL 的完整元数据，再次点击收起。
      -->
      <div v-if="preview.entries.length" class="preview-list">
        <article v-for="entry in preview.entries" :key="entry.path" class="preview-row"
          :class="{ expanded: isEntryExpanded(entry.path) }" @click="toggleEntry(entry.path)">
          <div class="preview-row-line">
            <span class="preview-main">
              <b class="preview-name" :title="entryTitle(entry)">{{ entryTitle(entry) }}</b>
              <span class="preview-path mono" :title="entry.path">{{ entry.path }}</span>
            </span>
            <!--
              依赖项没有版本语义（读不出 BepInPlugin），右侧显示「依赖」而不是「—」：
              破折号只说明「字段为空」，依赖两个字说明「为什么为空」。
            -->
            <span class="preview-version mono">
              {{ entry.role === 'dependency' ? t('依赖') : (formatVersion(entry.version) || '—') }}
            </span>
          </div>
          <n-collapse-transition :show="isEntryExpanded(entry.path)" display-directive="show">
            <div class="preview-row-detail">
              <div class="preview-item"><span class="muted">{{ t('名称') }}</span><b>{{ entry.name || '—' }}</b></div>
              <div class="preview-item"><span class="muted">{{ t('版本') }}</span><b class="mono">{{ formatVersion(entry.version) || '—' }}</b></div>
              <div class="preview-item"><span class="muted">{{ t('标题') }}</span><b>{{ entry.title || '—' }}</b></div>
              <div class="preview-item"><span class="muted">{{ t('产品名') }}</span><b>{{ entry.product_name || '—' }}</b></div>
              <div class="preview-item"><span class="muted">{{ t('作者') }}</span><b>{{ entry.company || '—' }}</b></div>
              <div class="preview-item"><span class="muted">{{ t('依赖') }}</span>
                <b>{{ entry.role === 'dependency' ? t('是') : t('否') }}</b>
              </div>
              <div class="preview-item wide"><span class="muted">GUID</span><b class="mono">{{ entry.guid || '—' }}</b></div>
              <div class="preview-item wide"><span class="muted">{{ t('原始文件名') }}</span><b class="mono">{{ entry.original_filename || '—' }}</b></div>
              <div class="preview-item wide"><span class="muted">{{ t('文件') }}</span><b class="mono">{{ entry.path }}</b></div>
            </div>
          </n-collapse-transition>
        </article>
      </div>
      <div v-else class="muted">{{ t('这个压缩包里没有 DLL 文件') }}</div>


      <!--
        仓库来源才展示本次 Release 的提交日志：用户点「立即安装」之前
        应该能看到这次到底更新了什么。本地文件与直链没有版本概念，不显示。
      -->
      <!-- showReleaseLog 已保证正文非空，这里不需要「没有日志」的占位分支 -->
      <div v-if="showReleaseLog" class="release-log">
        <div class="release-log-head">
          <span class="muted">{{ t('提交日志') }}</span>
          <b class="mono">{{ formatVersion(preview.release_tag) }}</b>
        </div>
        <!-- renderReadme 关闭了原始 HTML，日志只能输出 Markdown 生成的安全节点 -->
        <div class="release-log-box" v-html="releaseLogHtml"></div>
      </div>
    </div>

    <template #footer>
      <n-button secondary :disabled="installing" @click="requestDiscard">{{ t('取消') }}</n-button>
      <n-button type="primary" :loading="installing" @click="confirm">{{ t('立即安装') }}</n-button>
    </template>
  </AppModal>
</template>

<style scoped>
.install-preview {
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.preview-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.preview-head b { font-size: 13.5px; color: var(--oc2-text-1); }
/*
 * 条目卡片：与 MOD 管理页的 .mod-card 同一套紧凑横排语言 ——
 * 圆角外边框、单行「名称居左 / 版本居右」，展开后追加详情网格。
 * 这里不放置开关、状态标签与操作按钮：安装前只需要确认内容。
 */
/*
 * 条目区不自己限高：展开详情本该完整可见，一旦内部再滚动，
 * 卡片下半截会被裁在容器里，看起来像渲染坏了。
 * 整体高度由弹窗内容区承担（见 .install-preview 的 max-height）。
 */
.preview-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.preview-row {
  display: flex;
  flex-direction: column;
  padding: 9px 12px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  background: var(--oc2-card);
  cursor: pointer;
  transition: border-color .16s ease, background-color .16s ease;
}
.preview-row:hover { border-color: var(--oc2-accent); background: var(--oc2-row-hover); }
.preview-row.expanded { border-color: var(--oc2-accent); }
.preview-row-line {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: 14px;
  min-width: 0;
}
/*
 * 左侧「插件名 + 路径」自己是一组：路径允许被压缩并省略，
 * 版本列 flex: 0 0 auto 永不缩，所以路径只会避让、不会顶开版本。
 */
.preview-main {
  display: flex;
  align-items: baseline;
  gap: 8px;
  min-width: 0;
  flex: 1 1 auto;
}
.preview-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--oc2-text-1);
  white-space: nowrap;
  flex: 0 1 auto;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}
.preview-path {
  font-size: 11.5px;
  color: var(--oc2-text-4);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  /* 路径是次要信息：优先让插件名保持完整 */
  flex: 1 1 auto;
  min-width: 0;
}
.preview-version { font-size: 12px; color: var(--oc2-text-3); flex: 0 0 auto; }
/* 展开后的元数据网格：与 MOD 管理页详情区同款两列布局 */
.preview-row-detail {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 16px;
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--oc2-row-line);
}
.preview-item { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.preview-item.wide { grid-column: 1 / -1; }
.preview-item span { font-size: 11px; }
.preview-item b {
  font-size: 12px;
  font-weight: 500;
  color: var(--oc2-text-1);
  overflow-wrap: anywhere;
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
/* 底部操作行：靠右 */
.install-actions { display: flex; justify-content: flex-end; gap: 8px; }
</style>
