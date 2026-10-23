<script setup lang="ts">
import { nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { FileText } from 'lucide-vue-next'
import { t } from '../i18n'
import MarkdownContent from './MarkdownContent.vue'
import { repositoryMarkdownBaseUrl } from '../markdown'
import type { Repository } from '../types'

const props = defineProps<{ repository: Repository; note: string; community: boolean }>()
const emit = defineEmits<{ note: []; readme: [] }>()
// Cards show at most seven lines; only taller notes get the detail button.
const NOTE_MAX_LINES = 7
const measurement = ref<HTMLElement | null>(null)
const long = ref(false)
const clampHeight = ref(0)
let observer: ResizeObserver | undefined

function measure() {
  const element = measurement.value
  if (!element) {
    long.value = false
    return
  }
  const style = getComputedStyle(element)
  const fontSize = Number.parseFloat(style.fontSize) || 11
  const lineHeight = Number.parseFloat(style.lineHeight) || fontSize * 1.45
  const limit = lineHeight * NOTE_MAX_LINES
  clampHeight.value = Math.round(limit)
  // scrollHeight still reports the full content while the box is clamped.
  long.value = element.scrollHeight > limit + 1
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
    <div v-show="note" ref="measurement" class="repo-note"
      :class="{ 'repo-note-clamped': !community && long }"
      :style="!community && long ? { maxHeight: `${clampHeight}px` } : undefined">
      <MarkdownContent :source="note" :base-url="repositoryMarkdownBaseUrl(repository)" breaks @layout="measure" />
    </div>
    <div class="repo-document-actions">
      <button v-if="!community && note && long" class="repo-note-button" @click="emit('note')">
        <FileText :size="13" />{{ t('查看 MOD 简介') }}
      </button>
      <button class="repo-note-button" @click="emit('readme')"><FileText :size="13" />{{ t('查看仓库说明') }}</button>
    </div>
  </div>
</template>
