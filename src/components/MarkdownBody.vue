<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { renderMermaidBlocks, renderReadme } from '../markdown'

const props = defineProps<{ source: string }>()
const body = ref<HTMLElement | null>(null)
const html = computed(() => renderReadme(props.source))

// The ref also changes on teleport/tab mount, even when the Markdown is unchanged.
watch([body, html], ([element]) => void renderMermaidBlocks(element), { flush: 'post' })
</script>

<template>
  <div ref="body" class="markdown-body" v-html="html"></div>
</template>
