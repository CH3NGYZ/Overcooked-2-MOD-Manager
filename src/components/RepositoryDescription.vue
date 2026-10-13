<script setup lang="ts">
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { FileText } from 'lucide-vue-next'
import { t } from '../i18n'
import { markdownLinkUrl, renderMarkdown, repositoryMarkdownBaseUrl } from '../markdown'
import { api } from '../services/tauri'
import type { Repository } from '../types'

const props = defineProps<{ repository: Repository; note: string; community: boolean }>()
const emit = defineEmits<{ note: []; readme: [] }>()
const measurement = ref<HTMLElement | null>(null)
const long = ref(false)
const linkError = ref('')
const contentHtml = computed(() => renderMarkdown(props.note, repositoryMarkdownBaseUrl(props.repository)))
let observer: ResizeObserver | undefined

function measure() {
  const element = measurement.value
  const limit = Number.parseFloat(element ? getComputedStyle(element).getPropertyValue('--repository-note-limit') : '') || 96
  long.value = !!element && element.scrollHeight > limit
}

async function openLink(event: MouseEvent) {
  const url = markdownLinkUrl(event)
  if (!url) return
  linkError.value = ''
  try { await api.openReleasePage(url) }
  catch (reason) { linkError.value = reason instanceof Error ? reason.message : String(reason) }
}

watch(contentHtml, () => { linkError.value = ''; void nextTick(measure) }, { flush: 'post' })
onMounted(() => {
  observer = new ResizeObserver(measure)
  if (measurement.value) observer.observe(measurement.value)
  measure()
})
onUnmounted(() => observer?.disconnect())
</script>

<template>
  <div class="repo-description">
    <div ref="measurement" class="repo-note markdown-body repo-note-measure" aria-hidden="true" inert
      @load.capture="measure" @error.capture="measure" v-html="contentHtml"></div>
    <div v-if="note && (community || !long)" class="repo-note markdown-body" @click="openLink" v-html="contentHtml"></div>
    <p v-if="linkError" class="danger" role="alert">{{ t(linkError) }}</p>
    <div class="repo-document-actions">
      <button v-if="!community && note && long" class="repo-note-button" @click="emit('note')">
        <FileText :size="13" />{{ t('查看 MOD 简介') }}
      </button>
      <button class="repo-note-button" @click="emit('readme')"><FileText :size="13" />{{ t('查看仓库说明') }}</button>
    </div>
  </div>
</template>
