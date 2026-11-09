<script setup lang="ts">
import { computed } from 'vue'
import { t } from '../../i18n'
import type { LevelPreviewMenuTile } from '../../types'
const props=defineProps<{tiles:LevelPreviewMenuTile[]; index:number; ancestors?:number[]}>()
const tile=computed(()=>props.tiles[props.index])
const path=computed(()=>[...(props.ancestors ?? []),props.index])
const children=computed(()=>tile.value?.children.filter(i=>Number.isInteger(i) && i>=0 && i<props.tiles.length && !path.value.includes(i) && path.value.length<8) ?? [])
</script>
<template>
  <div v-if="tile" class="recipe-branch">
    <div class="menu-tile" :class="{'finished-tile':index===0}">
      <div class="menu-preparation">
      <div class="menu-pictures" :aria-label="index===0 ? t('levelPreview.recipes') : t('levelPreview.ingredients')">
        <span v-for="(icon,i) in tile.pictures" :key="i" class="menu-icon" :title="icon.name || t('levelPreview.unresolved')">
          <img v-if="icon.data_url" :src="icon.data_url" :alt="icon.name" loading="lazy" /><span v-else class="missing-icon" :aria-label="t('levelPreview.unresolved')">?</span>
        </span>
      </div>
      <div v-if="index!==0 && children.length" class="ingredient-groups"><RecipeTileGroup v-for="(child,i) in children" :key="`${child}-${i}`" :tiles="tiles" :index="child" :ancestors="path" /></div>
      </div>
      <div v-if="tile.modifiers.length" class="menu-steps" :aria-label="t('levelPreview.steps')">
        <span v-for="(icon,i) in tile.modifiers" :key="i" class="menu-icon" :title="icon.name || t('levelPreview.unresolved')">
          <img v-if="icon.data_url" :src="icon.data_url" :alt="icon.name" loading="lazy" /><span v-else class="missing-icon" :aria-label="t('levelPreview.unresolved')">?</span>
        </span>
      </div>
    </div>
    <div v-if="index===0 && children.length" class="menu-children"><RecipeTileGroup v-for="(child,i) in children" :key="`${child}-${i}`" :tiles="tiles" :index="child" :ancestors="path" /></div>
  </div>
</template>
<style scoped>
.recipe-branch{display:flex;flex-direction:column;align-items:center;gap:8px;min-width:0;max-width:100%;}
.menu-tile{border:1px solid var(--oc2-row-line);border-radius:10px;background:var(--oc2-sunken);padding:4px;min-width:0;max-width:100%;}
.menu-preparation,.ingredient-groups{display:flex;flex-wrap:nowrap;justify-content:center;align-items:flex-start;gap:clamp(5px,2cqw,20px);max-width:100%;}
.menu-pictures,.menu-steps{display:flex;flex-wrap:wrap;justify-content:center;gap:3px;}
.menu-steps{flex-wrap:nowrap;gap:clamp(5px,2cqw,20px);background:#f5f1e7;border-radius:6px;border-top:1px solid var(--oc2-row-line);margin-top:5px;padding-top:5px;}
.menu-icon{display:inline-flex;width:clamp(24px,16cqw,34px);height:clamp(24px,16cqw,34px);align-items:center;justify-content:center;flex:none;}
.menu-icon img{width:100%;height:100%;object-fit:contain;}
.finished-tile{background:transparent;border:0;}.finished-tile .menu-icon{width:72px;height:72px;}
.menu-children{display:flex;flex-wrap:nowrap;align-items:flex-start;justify-content:center;gap:clamp(5px,2cqw,20px);max-width:100%;padding-top:8px;border-top:1px solid var(--oc2-row-line);}
.missing-icon{color:var(--oc2-text-3);border:1px dashed var(--oc2-row-line);border-radius:50%;width:26px;height:26px;text-align:center;line-height:26px;}
</style>
