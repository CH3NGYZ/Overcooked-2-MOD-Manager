<script setup lang="ts">
import { t } from '../i18n'
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton } from 'naive-ui'
import { api, watchAppUpdatePhase } from '../services/tauri'
import { renderMermaidBlocks, renderReadme } from '../markdown'
import AppModal from './AppModal.vue'
import type { AppUpdateInfo } from '../types'

const props = defineProps<{ show: boolean; update: AppUpdateInfo | null }>()
const emit = defineEmits<{ 'update:show': [value: boolean] }>()

const message = useMessage()
const installing = ref(false)
const phaseText = ref('')

let unlisten: (() => void) | null = null
void watchAppUpdatePhase((event) => {
  if (event.message) phaseText.value = event.message
}).then((un) => { unlisten = un })
onUnmounted(() => { unlisten?.() })

const bodyText = computed(() => props.update?.body?.trim() || t('本次更新没有提供说明。'))
/** 更新日志按 Markdown 渲染（与 note/README 同一函数）。 */
const bodyHtml = computed(() => renderReadme(props.update?.body?.trim() || ''))
/** 更新日志里也可能有 mermaid 图，渲染后补一次 SVG 转换。 */
const bodyElement = ref<HTMLElement | null>(null)
watch(
  [bodyHtml, () => props.show],
  async () => {
    await nextTick()
    if (props.show) void renderMermaidBlocks(bodyElement.value)
  },
  { flush: 'post' },
)

function close() {
  emit('update:show', false)
}

async function install() {
  installing.value = true
  phaseText.value = '准备下载更新…'
  try {
    const result = await api.installAppUpdate()
    if (result.ok) {
      message.success(result.message || '更新安装已开始，应用将自动退出')
      phaseText.value = ''
      close()
    } else {
      message.error(result.message)
      phaseText.value = ''
    }
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
    phaseText.value = ''
  } finally {
    installing.value = false
  }
}
</script>

<template>
  <!-- 复用统一弹窗外壳：尺寸随内容自适应，更新日志再长也不会超出视口 -->
  <AppModal :show="show" :mask-closable="false" max-width="min(520px, calc(100vw - 80px))"
    max-height="85vh" min-height="160px" @update:show="close">
    <div class="update-body">
      <h2 class="title">
        发现新版本<span v-if="update">: v{{ update.version }}</span>
      </h2>
      <div v-if="update" class="version-line muted">
        当前 v{{ update.current_version }} → 新版 v{{ update.version }}
        <span v-if="update.provider">（{{ update.provider === 'gitee' ? 'Gitee 渠道' : 'GitHub 渠道' }}）</span>
      </div>
      <div class="body-label muted">{{ t('更新内容：') }}</div>
      <div v-if="bodyHtml" ref="bodyElement" class="body markdown-body" v-html="bodyHtml"></div>
      <div v-else class="body">{{ bodyText }}</div>
      <div v-if="phaseText" class="phase">{{ phaseText }}</div>
    </div>
    <template #footer>
      <n-button quaternary :disabled="installing" @click="close">{{ t('取消') }}</n-button>
      <n-button type="primary" :loading="installing" @click="install">{{ t('更新') }}</n-button>
    </template>
  </AppModal>
</template>

<style scoped>
/* 外壳（卡片边框/内边距/滚动）由 AppModal 负责，这里只管内容排版 */
.update-body {
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.title {
  margin: 0 0 6px;
  font-size: 21px;
  font-weight: 700;
}
.version-line { margin-bottom: 12px; }
.body-label { margin-bottom: 6px; }
/*
 * 正文不再自己限高滚动：外层 AppModal 已经统一滚动，
 * 这里再套一层会出现两级滚动，手势体验很差。
 */
.body {
  white-space: pre-wrap;
  padding: 10px 12px;
  border: 1px solid var(--oc2-divider);
  border-radius: 8px;
  background: var(--oc2-sunken);
  font-size: 13px;
  line-height: 1.6;
  color: var(--oc2-text-2);
}
/* Markdown 渲染时交给块级排版，不要再叠加 pre-wrap。 */
.body.markdown-body {
  white-space: normal;
  overflow-wrap: anywhere;
}
.body.markdown-body :deep(h1),
.body.markdown-body :deep(h2),
.body.markdown-body :deep(h3) {
  margin: 12px 0 6px;
  font-size: 14px;
  color: var(--oc2-text-1);
}
.body.markdown-body :deep(p) { margin: 0 0 8px; }
.body.markdown-body :deep(ul),
.body.markdown-body :deep(ol) { margin: 0 0 8px; padding-left: 20px; }
.body.markdown-body :deep(code) {
  padding: 1px 4px;
  border-radius: 4px;
  background: var(--oc2-row-line);
  font-family: Consolas, monospace;
  font-size: 12px;
}
.body.markdown-body :deep(pre) {
  margin: 0 0 8px;
  padding: 8px 10px;
  border-radius: 6px;
  background: var(--oc2-card);
  overflow-x: auto;
}
.body.markdown-body :deep(pre code) { padding: 0; background: none; }
.body.markdown-body :deep(a) { color: var(--oc2-accent); }
.phase {
  margin-top: 10px;
  color: var(--oc2-accent);
  font-size: 12.5px;
}
</style>
