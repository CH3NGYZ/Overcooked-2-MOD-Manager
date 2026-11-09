<script setup lang="ts">
import { t } from '../../i18n'
import type { LevelPreviewRecipe } from '../../types'
import RecipeTileGroup from './RecipeTileGroup.vue'
defineProps<{ recipe: LevelPreviewRecipe; index: number }>()
defineEmits<{ select: [] }>()
</script>
<template>
  <button type="button" class="recipe-card" :class="{ 'is-inactive': !recipe.active }" :aria-label="`${t('levelPreview.details')} · ${recipe.name || `${t('levelPreview.unresolved')} #${index + 1}`}`" @click="$emit('select')">
    <span class="recipe-number">#{{ index + 1 }}</span>
    <span v-if="recipe.score !== null" class="recipe-score" :title="t('levelPreview.score')">{{ recipe.score }}</span>
    <RecipeTileGroup v-if="recipe.tiles?.length" :tiles="recipe.tiles" :index="0" />
    <img v-else-if="recipe.icon" class="finished-icon" :src="recipe.icon" :alt="recipe.name" />
    <span v-else class="unresolved-recipe" :title="recipe.name || t('levelPreview.unresolved')">?</span>
  </button>
</template>
<style scoped>
.recipe-card {container-type:inline-size;position:relative;display:flex;align-items:flex-start;justify-content:center;padding:28px 8px 12px;border:1px solid color-mix(in srgb,var(--oc2-text-3) 65%,transparent);border-radius:10px;min-width:0;background:transparent;color:inherit;cursor:pointer;font:inherit;}
.recipe-number,.recipe-score {position:absolute;top:7px;font-size:12px;font-weight:400;color:var(--oc2-text-3);font-variant-numeric:tabular-nums;}
.recipe-number {left:10px;}.recipe-score {right:10px;}
.recipe-card:hover {background:var(--oc2-sunken);border-color:var(--oc2-text-3);}
.recipe-card:focus-visible {outline:2px solid var(--oc2-text-2);outline-offset:2px;}
.finished-icon {width:72px;height:72px;object-fit:contain;}
.unresolved-recipe {display:grid;place-items:center;width:72px;height:72px;color:var(--oc2-text-3);font-size:24px;}
.is-inactive {opacity:.6;}
</style>
