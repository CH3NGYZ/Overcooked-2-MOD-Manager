<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { FileText } from 'lucide-vue-next'
import { t } from '../i18n'
import { renderMarkdown } from '../markdown'

const props = defineProps<{ note: string; community: boolean }>()
const emit = defineEmits<{ note: []; readme: [] }>()
const measurement = ref<HTMLElement | null>(null)
const long = ref(false)
let observer: ResizeObserver | undefined

function measure() {
  const element = measurement.value
  const limit = Number.parseFloat(element ? getComputedStyle(element).getPropertyValue('--repository-note-limit') : '') || 96
  long.value = !!element && element.scrollHeight > limit
}

watch(() => props.note, () => { void nextTick(measure) }, { flush: 'post' })
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
      v-html="renderMarkdown(note)"></div>
    <div v-if="note && (community || !long)" class="repo-note markdown-body" v-html="renderMarkdown(note)"></div>
    <div class="repo-document-actions">
      <button v-if="!community && note && long" class="repo-note-button" @click="emit('note')">
        <FileText :size="13" />{{ t('查看 MOD 简介') }}
      </button>
      <button class="repo-note-button" @click="emit('readme')"><FileText :size="13" />{{ t('查看仓库说明') }}</button>
    </div>
  </div>
</template>
