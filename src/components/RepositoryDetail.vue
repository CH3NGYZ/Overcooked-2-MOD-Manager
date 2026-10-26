<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from 'vue'
import { NButton, NTag } from 'naive-ui'
import { CloseOutline, DownloadOutline, LinkOutline, OpenOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { t, locale } from '../i18n'
import { renderReadme } from '../markdown'
import type { Repository, RepositoryReadme } from '../types'

const props = defineProps<{
  repo: Repository | null
  installing?: boolean
  gameBlocked?: boolean
}>()
const emit = defineEmits<{ close: []; install: []; back: [] }>()

const readme = ref<RepositoryReadme | null>(null)
const loading = ref(false)
const error = ref('')
let requestId: string | null = null

function pickI18n(map: Record<string, string> | undefined, fallback: string) {
  if (!map) return fallback
  return map[locale.value] || map['zh-CN'] || Object.values(map)[0] || fallback
}

const name = computed(() => (props.repo ? pickI18n(props.repo.name_i18n, props.repo.name) : ''))
const note = computed(() => (props.repo ? pickI18n(props.repo.note_i18n, props.repo.note || '') : ''))
const release = computed(() => props.repo?.cached_releases[0] ?? null)
const version = computed(() => release.value?.version || release.value?.tag_name || '—')
const downloads = computed(() => {
  const total = release.value?.assets.reduce((sum, asset) => sum + (asset.download_count ?? 0), 0) ?? 0
  return total || null
})
const updated = computed(() => release.value?.published_at?.slice(0, 10) || '—')

/** 许可证：README 常见 `License: MIT` / `## License` 段落首行，取不到就显示 —。 */
const license = computed(() => {
  const text = readme.value?.content ?? ''
  const inlineMatch = text.match(/licen[cs]e[^\n:：]*[:：]\s*([A-Za-z0-9.\-+ ]{2,24})/i)
  if (inlineMatch) return inlineMatch[1].trim()
  const section = text.match(/##+\s*licen[cs]e\s*\n+([^\n#]{2,40})/i)
  if (section) return section[1].trim()
  return '—'
})

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
const html = computed(() => renderReadme(readme.value?.content ?? ''))
/** 备注同样按 Markdown 渲染（note_i18n 已按当前语言取值）。 */
const noteHtml = computed(() => renderReadme(note.value))

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

watch(() => props.repo?.id, () => { void load() }, { immediate: true })
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
          <small class="muted">{{ repo.provider === 'gitee' ? 'Gitee' : 'GitHub' }} · {{ repo.username }}/{{ repo.repository }}</small>
        </div>
      </div>
      <div class="head-acts">
        <n-button type="primary" :loading="installing" :disabled="gameBlocked" @click="emit('install')">
          <DownloadOutline width="15" height="15" />{{ t('安装') }}
        </n-button>
        <n-button quaternary @click="emit('close')">
          <CloseOutline width="16" height="16" />
        </n-button>
      </div>
    </header>

    <!-- 指标网格：版本 / 下载量 / 许可证 / 更新于 / 标识 -->
    <div class="metrics">
      <div class="metric">
        <span class="muted">{{ t('最新版本') }}</span>
        <b class="mono">{{ version }}</b>
      </div>
      <div class="metric">
        <span class="muted">{{ t('下载量') }}</span>
        <b>{{ downloads?.toLocaleString() ?? '—' }}</b>
      </div>
      <div class="metric">
        <span class="muted">{{ t('许可证') }}</span>
        <b>{{ license }}</b>
      </div>
      <div class="metric">
        <span class="muted">{{ t('最后更新') }}</span>
        <b>{{ updated }}</b>
      </div>
      <div class="metric">
        <span class="muted">{{ t('标识') }}</span>
        <b class="mono">{{ repo.username }}/{{ repo.repository }}</b>
      </div>
    </div>

    <div class="links">
      <n-button size="small" secondary :disabled="!releaseUrl" @click="openUrl(releaseUrl)">
        <LinkOutline width="14" height="14" />{{ t('Release') }}
      </n-button>
      <n-button size="small" secondary :disabled="!sourceUrl" @click="openUrl(sourceUrl)">
        <OpenOutline width="14" height="14" />{{ t('源码') }}
      </n-button>
      <n-tag v-if="repo.download_mode === 'repository_file'" size="small" :bordered="false">{{ t('仓库文件') }}</n-tag>
      <n-tag v-if="!repo.enabled" size="small" :bordered="false">{{ t('未启用') }}</n-tag>
    </div>

    <hr class="divider" />

    <!-- 来自仓库源的备注 -->
    <section v-if="note" class="note markdown-body" v-html="noteHtml"></section>
    <section v-else class="note muted">{{ t('该仓库没有备注') }}</section>

    <hr class="divider" />

    <!-- README：加载中显示骨架动画 -->
    <section class="readme">
      <div v-if="loading" class="readme-loading" role="status" :aria-label="t('正在加载仓库说明')">
        <div class="skeleton-line w-95"></div>
        <div class="skeleton-line w-60"></div>
        <div class="skeleton-line w-80"></div>
        <div class="skeleton-line w-45"></div>
        <div class="loading-hint">{{ t('正在加载仓库说明…') }}</div>
      </div>
      <div v-else-if="error" class="muted">{{ error }}</div>
      <div v-else-if="!readme?.exists" class="muted">{{ t('文件不存在') }}</div>
      <div v-else class="markdown-body" v-html="html"></div>
    </section>
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
.head-acts { display: flex; align-items: center; gap: 6px; flex: 0 0 auto; }

.metrics {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(120px, 1fr));
  gap: 1px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  overflow: hidden;
  background: var(--oc2-row-line);
}
.metric {
  display: flex;
  flex-direction: column;
  gap: 3px;
  padding: 10px 12px;
  background: var(--oc2-sunken);
  min-width: 0;
}
.metric b {
  font-size: 13px;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.links { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.divider {
  margin: 2px 0;
  border: 0;
  border-top: 1px solid var(--oc2-row-line);
}
/* 备注区与 README 用同一套 Markdown 样式，这里只补外层间距。 */
.note {
  margin: 0;
}

/* README 骨架动画 */
.readme { min-height: 80px; }
.readme-loading { display: flex; flex-direction: column; gap: 10px; position: relative; }
.skeleton-line {
  height: 10px;
  border-radius: 6px;
  background: linear-gradient(90deg,
    var(--oc2-row-line) 0%,
    var(--oc2-row-hover) 40%,
    var(--oc2-row-line) 80%);
  background-size: 220% 100%;
  animation: oc2-skeleton 1.25s ease-in-out infinite;
}
.w-95 { width: 95%; }
.w-80 { width: 80%; }
.w-60 { width: 60%; }
.w-45 { width: 45%; }
.loading-hint { color: var(--oc2-text-3); font-size: 12px; }
@keyframes oc2-skeleton {
  0% { background-position: 120% 0; }
  100% { background-position: -60% 0; }
}
@media (prefers-reduced-motion: reduce) {
  .skeleton-line { animation: none; }
}

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
.markdown-body :deep(p) { margin: 0 0 10px; }
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
</style>
