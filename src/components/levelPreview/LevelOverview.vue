<script setup lang="ts">
import { onBeforeUnmount, ref, watch } from 'vue'
import { NButton, NPopover } from 'naive-ui'
import { t } from '../../i18n'
import type { LevelPreview, LevelPreviewPicture } from '../../types'
import LevelName from './LevelName.vue'
import PreviewPicture from './PreviewPicture.vue'
const props = defineProps<{ level: LevelPreview; name: string; picture: LevelPreviewPicture | null; loading: boolean }>()
const enlarged = ref(false)
const bundlesExpanded = ref(false)
const bundlesOverflow = ref(false)
const bundleList = ref<HTMLElement | null>(null)
let observer: ResizeObserver | undefined
function measureBundles() {
  const node = bundleList.value
  bundlesOverflow.value = !!node && node.scrollHeight > parseFloat(getComputedStyle(node).lineHeight) * 3 + 1
}
watch(bundleList, node => {
  observer?.disconnect()
  if (node) {
    observer = new ResizeObserver(measureBundles)
    observer.observe(node)
    measureBundles()
  }
}, { flush: 'post' })
watch(() => props.level, () => {
  enlarged.value = false
  bundlesExpanded.value = false
  measureBundles()
}, { flush: 'post' })
onBeforeUnmount(() => observer?.disconnect())
</script>
<template>
  <div class="level-information">
    <h2><LevelName :text="name" /></h2>
    <table class="level-information-table">
      <colgroup><col class="thumbnail-column" /><col class="label-column" /><col /><col class="label-column" /><col /></colgroup>
      <tbody>
        <tr>
          <td rowspan="5" class="thumbnail-cell">
            <n-popover v-model:show="enlarged" trigger="hover" placement="right" :disabled="!picture?.data_url" :show-arrow="false">
              <template #trigger><button class="thumbnail-trigger" :aria-label="name" @focus="enlarged=true" @blur="enlarged=false" @keydown.esc="enlarged=false"><PreviewPicture :picture="picture" :loading="loading" :alt="name" /></button></template>
              <div class="enlarged-thumbnail"><PreviewPicture :picture="picture" :alt="name" /></div>
            </n-popover>
            <small v-if="picture?.data_url">{{ picture.width }} × {{ picture.height }}</small>
          </td>
          <th scope="row">{{ t('levelPreview.nameEnglish') }}</th><td>{{ level.name || '—' }}</td>
          <th scope="row">{{ t('levelPreview.nameChinese') }}</th><td>{{ level.name_zh || '—' }}</td>
        </tr>
        <tr><th scope="row">{{ t('levelPreview.scene') }}</th><td>{{ level.scene_name || '—' }}</td><th scope="row">{{ t('levelPreview.file') }}</th><td :class="{'missing-file':!level.scene_exists}">{{ t(level.scene_exists ? 'levelPreview.exists' : 'levelPreview.absent') }}</td></tr>
        <tr><th scope="row">{{ t('levelPreview.minOrders') }}</th><td>{{ level.min_order_count }}</td><th scope="row">{{ t('levelPreview.maxOrders') }}</th><td>{{ level.max_order_count }}</td></tr>
        <tr><th scope="row">{{ t('levelPreview.totalRecipes') }}</th><td>{{ level.recipe_count }}</td><th scope="row">{{ t('levelPreview.activeRecipes') }}</th><td>{{ level.active_recipe_count }}</td></tr>
        <tr><th scope="row">{{ t('levelPreview.dependencies') }} · {{ level.dependencies.length }}</th><td colspan="3"><template v-if="level.dependencies.length"><ul ref="bundleList" class="bundle-list" :class="{'is-collapsed':!bundlesExpanded}"><li v-for="bundle in level.dependencies" :key="bundle">{{ bundle }}</li></ul><n-button v-if="bundlesOverflow" class="bundle-toggle" size="tiny" quaternary :aria-expanded="bundlesExpanded" @click="bundlesExpanded=!bundlesExpanded">{{ t(bundlesExpanded ? 'levelPreview.collapseBundles' : 'levelPreview.expandBundles') }}</n-button></template><span v-else>{{ t('levelPreview.none') }}</span></td></tr>
      </tbody>
    </table>
  </div>
</template>
<style scoped>
h2 {margin:0 0 8px;min-width:0;}
.level-information-table {width:100%;table-layout:fixed;border-collapse:collapse;font-size:12px;}
th,td {border:1px solid var(--oc2-row-line);padding:5px 8px;text-align:left;overflow-wrap:anywhere;}
th {font-weight:normal;color:var(--oc2-text-3);background:var(--oc2-sunken);}
.label-column {width:17%;}.thumbnail-column {width:22%;}
.bundle-list {margin:0;padding-left:16px;line-height:18px;}
.bundle-list.is-collapsed {max-height:54px;overflow:hidden;}
.thumbnail-cell small {display:block;color:var(--oc2-text-3);margin-top:4px;}
.thumbnail-cell {vertical-align:middle;text-align:center;padding:6px;}
.thumbnail-trigger {display:block;width:100%;max-width:160px;margin:auto;padding:0;border:0;border-radius:8px;background:transparent;cursor:zoom-in;}
.thumbnail-trigger:focus-visible {outline:2px solid var(--oc2-text-2);outline-offset:3px;}
.enlarged-thumbnail {width:min(524px,65vw,calc((100vh - 160px) * 524 / 308));aspect-ratio:524 / 308;}
.enlarged-thumbnail :deep(.preview-picture) {width:100%;height:100%;aspect-ratio:auto;}
.missing-file {color:var(--oc2-tone-critical-text);}
</style>
