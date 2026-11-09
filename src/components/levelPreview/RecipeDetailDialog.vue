<script setup lang="ts">
import { computed, ref, shallowRef } from 'vue'
import { t } from '../../i18n'
import { translateBackendMessage } from '../../locale/backend'
import type { LevelPreviewRecipe } from '../../types'
import AppModal from '../AppModal.vue'
import RecipeTileGroup from './RecipeTileGroup.vue'
const show = ref(false)
const recipe = shallowRef<LevelPreviewRecipe | null>(null)
const index = ref(0)
const title = computed(() => recipe.value?.name || `${t('levelPreview.unresolved')} #${index.value + 1}`)
function open(value: LevelPreviewRecipe, position: number) { recipe.value = value; index.value = position; show.value = true }
function afterLeave() { if (!show.value) recipe.value = null }
defineExpose({ open, close: () => { show.value = false } })
</script>
<template>
  <AppModal v-model:show="show" :title="title" max-width="min(560px, 90vw)" card-class="recipe-detail-modal" @after-leave="afterLeave">
    <div v-if="recipe" class="recipe-detail">
      <RecipeTileGroup v-if="recipe.tiles?.length" :tiles="recipe.tiles" :index="0" />
      <img v-else-if="recipe.icon" class="finished-icon" :src="recipe.icon" :alt="recipe.name" />
      <dl>
        <dt>{{ t('levelPreview.score') }}</dt><dd>{{ recipe.score ?? '—' }}</dd>
        <dt>{{ t('levelPreview.status') }}</dt><dd>{{ t(recipe.active ? 'levelPreview.active' : 'levelPreview.inactive') }}</dd>
        <template v-if="recipe.ingredients.length"><dt>{{ t('levelPreview.ingredients') }}</dt><dd>{{ recipe.ingredients.join(' · ') }}</dd></template>
        <template v-if="recipe.source"><dt>{{ t('levelPreview.source') }}</dt><dd>{{ recipe.source }}</dd></template>
      </dl>
      <p v-if="!recipe.resolved">{{ t('levelPreview.unresolved') }}</p>
      <p v-if="recipe.error" role="alert">{{ translateBackendMessage(recipe.error) }}</p>
    </div>
  </AppModal>
</template>
<style scoped>
.recipe-detail {display:grid;gap:20px;overflow-wrap:anywhere;}
.finished-icon {width:72px;height:72px;object-fit:contain;justify-self:center;}
dl {display:grid;grid-template-columns:auto minmax(0,1fr);gap:10px 16px;margin:0;}
dt {color:var(--oc2-text-3);}dd {margin:0;white-space:pre-wrap;}
</style>
