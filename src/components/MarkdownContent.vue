<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, ref, useId, watch } from 'vue'
import { t } from '../i18n'
import { markdownLinkUrl, renderMarkdown } from '../markdown'
import { enhanceMarkdown } from '../markdownEnhancements'
import { api } from '../services/tauri'

const props = defineProps<{ source: string; baseUrl?: string }>()
const emit = defineEmits<{ layout: [] }>()
const element = ref<HTMLElement | null>(null)
const idPrefix = `markdown-${useId()}-`
const html = computed(() => renderMarkdown(props.source, props.baseUrl, idPrefix))
const linkError = ref('')
let revision = 0
let controller: AbortController | undefined

function layout() { if (!controller?.signal.aborted) emit('layout') }

watch(html, async () => {
  const current = ++revision
  controller?.abort()
  controller = new AbortController()
  linkError.value = ''
  await nextTick()
  if (current !== revision || !element.value) return
  layout()
  await enhanceMarkdown(element.value, {
    signal: controller.signal,
    diagramLabel: t('Mermaid 图表'),
    diagramError: t('Mermaid 图表无法渲染，已保留源码。请检查语法、大小或不受支持的配置。'),
    onLayout: () => { if (current === revision) layout() },
  })
}, { immediate: true, flush: 'post' })

async function openLink(event: MouseEvent) {
  const url = markdownLinkUrl(event)
  if (!url) return
  const current = revision
  try { await api.openReleasePage(url) }
  catch (reason) {
    if (current === revision) linkError.value = reason instanceof Error ? reason.message : String(reason)
  }
}

onBeforeUnmount(() => { revision += 1; controller?.abort() })
</script>

<template>
  <div class="markdown-content">
    <div ref="element" class="markdown-body" :data-markdown-prefix="idPrefix" @click="openLink" @load.capture="layout" @error.capture="layout" v-html="html"></div>
    <p v-if="linkError" class="danger" role="alert">{{ t(linkError) }}</p>
  </div>
</template>
