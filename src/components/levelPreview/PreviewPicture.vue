<script setup lang="ts">
import { NSpin } from 'naive-ui'
import { t } from '../../i18n'
import type { LevelPreviewPicture } from '../../types'
withDefaults(defineProps<{picture?: LevelPreviewPicture | null; loading?: boolean; alt: string}>(), {picture:null, loading:false})
</script>
<template>
  <div class="preview-picture" :aria-busy="loading">
    <n-spin v-if="loading" size="small" />
    <img v-else-if="picture?.data_url" :src="picture.data_url" :alt="alt" />
    <span v-else>{{ t(picture?.state === 'error' ? 'levelPreview.error' : 'levelPreview.missing') }}</span>
  </div>
</template>
<style scoped>
.preview-picture { display:grid; place-items:center; background:var(--oc2-sunken); border-radius:8px; aspect-ratio: 524 / 308; overflow:hidden; color:var(--oc2-text-3); }
img { display:block; width:100%; height:100%; object-fit:contain; }
</style>
