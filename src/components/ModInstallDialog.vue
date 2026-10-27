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
import { computed, ref } from 'vue'
import { NButton, NModal, NTag, useMessage } from 'naive-ui'
import { api } from '../services/tauri'
import { tagColorFor } from '../theme'
import { renderReadme } from '../markdown'
import { t } from '../i18n'
import type { ModPreview } from '../types'

const props = defineProps<{ show: boolean; preview: ModPreview | null }>()
const emit = defineEmits<{
  (event: 'update:show', value: boolean): void
  (event: 'installed'): void
  /** 用户放弃这份预览（取消按钮、叉掉），调用方据此清理暂存。 */
  (event: 'discarded', preview: ModPreview): void
}>()

const message = useMessage()
const installing = ref(false)

/**
 * 仓库来源才展示 Release 提交日志；本地文件与直链没有版本概念，整块不出现。
 * RAW 仓库虽然来源是仓库，但没有 Release，`release_tag` 为空，同样隐藏。
 */
const releaseLogHtml = computed(() =>
  props.preview?.origin === 'repository' ? renderReadme(props.preview.release_body ?? '') : '',
)
const showReleaseLog = computed(
  () => props.preview?.origin === 'repository' && props.preview.release_tag.trim() !== '',
)

/** 弹窗开关；关闭时统一走 discard，保证暂存一定被清理。 */
const visible = computed({
  get: () => props.show,
  set: (value: boolean) => {
    if (value) emit('update:show', true)
    else discard()
  },
})

/** 放弃当前预览：通知调用方清理，并收起面板。预览由调用方持有。 */
function discard() {
  const current = props.preview
  if (current) emit('discarded', current)
  emit('update:show', false)
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
    // 后端已在 confirm 里删掉暂存，这里只收起面板
    emit('update:show', false)
    emit('installed')
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    installing.value = false
  }
}
</script>

<template>
  <n-modal :show="visible" preset="card" :title="t('安装详情')" :bordered="false"
    style="width: min(720px, 92vw)" @update:show="(value: boolean) => { if (!value) discard() }">
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

      <!-- 单 mod：完整元数据 -->
      <div v-if="preview.kind === 'single'" class="preview-grid">
        <div class="preview-item"><span class="muted">{{ t('名称') }}</span><b>{{ preview.entries[0]?.name || '—' }}</b></div>
        <div class="preview-item"><span class="muted">GUID</span><b class="mono">{{ preview.entries[0]?.guid || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('版本') }}</span><b class="mono">{{ preview.entries[0]?.version || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('标题') }}</span><b>{{ preview.entries[0]?.title || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('产品名') }}</span><b>{{ preview.entries[0]?.product_name || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('作者') }}</span><b>{{ preview.entries[0]?.company || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('原始文件名') }}</span><b class="mono">{{ preview.entries[0]?.original_filename || '—' }}</b></div>
        <div class="preview-item"><span class="muted">{{ t('文件') }}</span><b class="mono">{{ preview.entries[0]?.path || '—' }}</b></div>
      </div>

      <!-- 包：逐 DLL 列表，依赖单独标注 -->
      <div v-else class="preview-list">
        <div v-for="entry in preview.entries" :key="entry.path" class="preview-row">
          <n-tag size="tiny" :bordered="false" class="state-tag"
            :color="tagColorFor(entry.role === 'dependency' ? 'muted' : 'accent')">
            {{ entry.role === 'dependency' ? t('依赖') : t('MOD') }}
          </n-tag>
          <b class="preview-name">{{ entry.name || entry.path }}</b>
          <span class="muted mono">{{ entry.version || '—' }}</span>
        </div>
        <div v-if="!preview.entries.length" class="muted">{{ t('这个压缩包里没有 DLL 文件') }}</div>
      </div>

      <!--
        仓库来源才展示本次 Release 的提交日志：用户点「立即安装」之前
        应该能看到这次到底更新了什么。本地文件与直链没有版本概念，不显示。
      -->
      <div v-if="showReleaseLog" class="release-log">
        <div class="release-log-head">
          <span class="muted">{{ t('提交日志') }}</span>
          <b class="mono">{{ preview.release_tag }}</b>
        </div>
        <!-- renderReadme 关闭了原始 HTML，日志只能输出 Markdown 生成的安全节点 -->
        <div v-if="releaseLogHtml" class="release-log-box" v-html="releaseLogHtml"></div>
        <div v-else class="release-log-box release-log-empty muted">{{ t('该版本没有提交日志') }}</div>
      </div>

      <div class="install-actions">
        <n-button secondary :disabled="installing" @click="discard">{{ t('取消') }}</n-button>
        <n-button type="primary" :loading="installing" @click="confirm">{{ t('立即安装') }}</n-button>
      </div>
    </div>
  </n-modal>
</template>

<style scoped>
/* 预览区：与详情区同款的紧凑信息密度 */
.install-preview { display: flex; flex-direction: column; gap: 12px; }
.preview-head {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.preview-head b { font-size: 13.5px; color: var(--oc2-text-1); }
.preview-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 16px;
}
.preview-item { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
.preview-item span { font-size: 11px; }
.preview-item b {
  font-size: 12px;
  font-weight: 500;
  color: var(--oc2-text-1);
  overflow-wrap: anywhere;
}
.preview-list { display: flex; flex-direction: column; gap: 6px; max-height: 280px; overflow-y: auto; }
.preview-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  align-items: center;
  gap: 10px;
  padding: 7px 10px;
  border-radius: 8px;
  background: var(--oc2-sunken);
}
.preview-name {
  font-size: 12.5px;
  font-weight: 500;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
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
.release-log-box {
  max-height: 180px;
  overflow-y: auto;
  padding: 8px 10px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 8px;
  background: var(--oc2-sunken);
  font-size: 12px;
  line-height: 1.65;
  overscroll-behavior: contain;
}
.release-log-empty {
  display: grid;
  place-items: center;
  min-height: 48px;
  font-size: 12px;
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
