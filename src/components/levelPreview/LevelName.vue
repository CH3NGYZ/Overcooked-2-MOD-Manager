<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
const props = withDefaults(defineProps<{ text: string; maximum?: number }>(), { maximum: 20 })
const element = ref<HTMLElement | null>(null)
let observer: ResizeObserver | undefined
function fit() {
  const node = element.value
  if (!node || !node.clientWidth) return
  let low = 10
  let high = props.maximum
  node.style.fontSize = `${high}px`
  if (node.scrollWidth <= node.clientWidth + 1) return
  while (high - low > 0.25) {
    const middle = (low + high) / 2
    node.style.fontSize = `${middle}px`
    if (node.scrollWidth > node.clientWidth + 1) high = middle
    else low = middle
  }
  node.style.fontSize = `${low}px`
}
watch(element, node => {
  observer?.disconnect()
  if (node) {
    observer = new ResizeObserver(fit)
    observer.observe(node)
    fit()
  }
}, { flush: 'post' })
watch(() => [props.text, props.maximum], fit, { flush: 'post' })
onBeforeUnmount(() => observer?.disconnect())
</script>
<template><span ref="element" class="level-name" :title="text">{{ text }}</span></template>
<style scoped>
.level-name {display:block;min-width:0;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;line-height:1.5;}
</style>
