<script setup lang="ts">
import { ref, watch } from 'vue'
import { NPopover } from 'naive-ui'
import { t } from '../../i18n'
import type { LevelPreview, LevelPreviewPicture } from '../../types'
import LevelName from './LevelName.vue'
import PreviewPicture from './PreviewPicture.vue'
const props = defineProps<{ level: LevelPreview; name: string; picture: LevelPreviewPicture | null; loading: boolean }>()
const enlarged = ref(false)
watch(() => props.level, () => { enlarged.value = false })
</script>
<template>
  <div class="level-information">
    <h2><LevelName :text="name" /></h2>
    <table class="level-information-table">
      <colgroup><col class="label-column" /><col /><col class="thumbnail-column" /></colgroup>
      <tbody>
        <tr><th scope="row">{{ t('levelPreview.name') }}</th><td>{{ level.name }}<template v-if="level.name_zh && level.name_zh !== name"><br />{{ level.name_zh }}</template></td>
          <td rowspan="4" class="thumbnail-cell">
            <n-popover v-model:show="enlarged" trigger="hover" placement="left" :disabled="!picture?.data_url" :show-arrow="false">
              <template #trigger><button class="thumbnail-trigger" :aria-label="name" @focus="enlarged=true" @blur="enlarged=false" @keydown.esc="enlarged=false"><PreviewPicture :picture="picture" :loading="loading" :alt="name" /></button></template>
              <div class="enlarged-thumbnail"><PreviewPicture :picture="picture" :alt="name" /></div>
            </n-popover>
          </td>
        </tr>
        <tr><th scope="row">{{ t('levelPreview.scene') }}</th><td>{{ level.scene_name }}</td></tr>
        <tr><th scope="row">{{ t('levelPreview.orders') }}</th><td>{{ level.min_order_count }}–{{ level.max_order_count }}</td></tr>
        <tr><th scope="row">{{ t('levelPreview.file') }}</th><td :class="{'missing-file':!level.scene_exists}">{{ t(level.scene_exists ? 'levelPreview.exists' : 'levelPreview.absent') }}</td></tr>
      </tbody>
    </table>
  </div>
</template>
<style scoped>
h2 {margin:0 0 8px;min-width:0;}
.level-information-table {width:100%;table-layout:fixed;border-collapse:collapse;font-size:12px;}
th,td {border:1px solid var(--oc2-row-line);padding:5px 8px;text-align:left;overflow-wrap:anywhere;}
th {font-weight:normal;color:var(--oc2-text-3);background:var(--oc2-sunken);}
.label-column {width:112px;}.thumbnail-column {width:28%;}
.thumbnail-cell {vertical-align:middle;text-align:center;padding:6px;}
.thumbnail-trigger {display:block;width:100%;max-width:160px;margin:auto;padding:0;border:0;border-radius:8px;background:transparent;cursor:zoom-in;}
.thumbnail-trigger:focus-visible {outline:2px solid var(--oc2-text-2);outline-offset:3px;}
.enlarged-thumbnail {width:min(524px,65vw,calc((100vh - 160px) * 524 / 308));aspect-ratio:524 / 308;}
.enlarged-thumbnail :deep(.preview-picture) {width:100%;height:100%;aspect-ratio:auto;}
.missing-file {color:var(--oc2-tone-critical-text);}
</style>
