<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { NButton, NInput, NSpin, useMessage } from 'naive-ui'
import { t, locale } from '../../i18n'
import { translateBackendMessage } from '../../locale/backend'
import { levelSetDisplayName } from '../../levelSetStatus'
import { useLevelPreview } from '../../composables/useLevelPreview'
import { CameraOutline } from '@vicons/ionicons5'
import HeaderIconAction from '../HeaderIconAction.vue'
import { prepareLevelScreenshot } from '../../levelScreenshot'
import { useElementScreenshot } from '../../composables/useElementScreenshot'
import AppModal from '../AppModal.vue'
import LevelOverview from './LevelOverview.vue'
import LevelName from './LevelName.vue'
import RecipeCard from './RecipeCard.vue'
import RecipeDetailDialog from './RecipeDetailDialog.vue'
import type { LevelSetInfo, LevelPreview } from '../../types'
const recipeDetail = ref<InstanceType<typeof RecipeDetailDialog> | null>(null)
const p = useLevelPreview()
const content = ref<HTMLElement | null>(null)
const message = useMessage()
const { copying, copy } = useElementScreenshot()
async function copyScreenshot() {
  if (!content.value || !target.value || !current.value) return
  const screenshotTitle = `${levelSetDisplayName(target.value, locale.value)} · ${name(current.value)}`
  try { await copy(content.value, { width: 760, pixelRatio: 1.5, prepare: snapshot => prepareLevelScreenshot(snapshot, screenshotTitle) }); message.success(t('common.action.copied')) }
  catch { message.error(t('levelPreview.screenshotFailed')) }
}
const {show,target,selected,query,loading,error,picture,recipes,pictureBusy,recipeBusy,pictureError,recipeError,current,visible} = p
const title = computed(() => target.value ? `${t('levelPreview.open')} · ${levelSetDisplayName(target.value,locale.value)}` : t('levelPreview.open'))
function name(level: LevelPreview) {return locale.value.startsWith('zh') ? level.name_zh || level.name : level.name || level.name_zh}
watch(show, value => { if (!value) recipeDetail.value?.close() })
defineExpose({open:(set:LevelSetInfo) => p.open(set)})
</script>
<template>
  <AppModal v-model:show="show" :title="title" fill :scrollable="false" max-width="90%" max-height="90%" card-class="level-preview-modal" @after-leave="p.afterLeave">
    <div v-if="error" class="preview-status" role="alert">{{ translateBackendMessage(error) }} <n-button @click="target && p.open(target)">{{ t('levelPreview.retry') }}</n-button></div>
    <div v-else class="preview-layout">
      <aside class="preview-sidebar">
        <n-input v-model:value="query" :placeholder="t('levelPreview.search')" :aria-label="t('levelPreview.search')" clearable />
        <nav class="level-list" :aria-label="t('levelPreview.open')">
          <button v-for="{level,index} in visible" :key="index" class="level-choice" :class="{'is-selected':selected===index}" :aria-current="selected===index ? 'true' : undefined" @click="selected=index">
            <span class="level-number">{{ String(index+1).padStart(2,'0') }}</span><span class="level-choice-label"><strong><LevelName :text="name(level)" :maximum="14" /></strong><small>{{ level.scene_name }}</small></span>
          </button>
          <p v-if="!loading && !visible.length">{{ t('levelPreview.noResults') }}</p>
        </nav>
      </aside>
      <section ref="content" class="preview-content" :aria-busy="loading">
        <n-spin v-if="loading && !current" size="small" />
        <template v-if="current">
          <LevelOverview :level="current" :name="name(current)" :picture="picture" :loading="pictureBusy"><template #actions><HeaderIconAction data-screenshot-exclude :label="t('levelPreview.copyScreenshot')" :loading="copying" :disabled="copying || pictureBusy || recipeBusy" @click="copyScreenshot"><CameraOutline /></HeaderIconAction></template></LevelOverview>
          <p v-if="pictureError" role="alert">{{ translateBackendMessage(pictureError) }}</p>
          <div class="recipe-toolbar"><span class="muted">{{ t('levelPreview.counts',{resolved:recipes?.filter(r=>r.resolved).length ?? 0,total:current.recipe_count,active:current.active_recipe_count}) }}</span><n-button size="tiny" quaternary @click="p.retry">{{ t('levelPreview.retry') }}</n-button></div>
          <n-spin v-if="recipeBusy" size="small" />
          <p v-else-if="recipeError" role="alert">{{ translateBackendMessage(recipeError) }}</p>
          <div v-else-if="recipes?.length" class="recipe-grid"><RecipeCard v-for="(recipe,index) in recipes" :key="index" :recipe="recipe" :index="index" @select="recipeDetail?.open(recipe,index)" /></div>
          <p v-else>{{ t('levelPreview.empty') }}</p>
        </template>
      </section>
    </div>
  </AppModal>
  <RecipeDetailDialog ref="recipeDetail" />
</template>
<style scoped>
.preview-layout {display:grid;grid-template-columns:220px minmax(0,1fr); flex:1; min-height:0; overflow:hidden; gap:20px;}
.preview-sidebar {display:flex;flex-direction:column;gap:12px;min-height:0;border-right:1px solid var(--oc2-row-line);padding-right:16px;}
.level-list {overflow-y:auto;min-height:0;display:flex;flex-direction:column;gap:6px;}
.level-choice {display:grid;grid-template-columns:3ch minmax(0,1fr);align-items:start;gap:10px;text-align:left;padding:12px 10px;border:1px solid transparent;border-radius:8px;background:transparent;color:var(--oc2-text-2);cursor:pointer;font:inherit;}
.level-choice:hover {background:var(--oc2-sunken);}
.level-choice.is-selected {background:var(--oc2-sunken);border-color:var(--oc2-text-3);}
.level-choice strong,.level-choice small {display:block;overflow-wrap:anywhere;}
.level-choice small,.level-number,.muted {color:var(--oc2-text-3);font-size:12px;}
.level-number {font-variant-numeric:tabular-nums;white-space:nowrap;line-height:1.6;padding-top:1px;text-align:right;}
.level-choice-label {min-width:0;line-height:1.6;}
.level-choice-label small {margin-top:3px;}
.preview-content {overflow-y:auto;min-height:0;padding-right:8px;scrollbar-gutter:stable;}
p {margin:6px 0 12px;}
.recipe-toolbar {display:flex;align-items:center;justify-content:space-between;gap:8px;margin-top:10px;}
.recipe-grid {display:grid;grid-template-columns:repeat(auto-fit,minmax(min(100%,180px),1fr));gap:10px;margin-top:12px;}
summary {cursor:pointer;} .preview-status {padding:20px;}
@media (max-width:850px) {.preview-layout{grid-template-columns:180px minmax(0,1fr);gap:12px;}}
</style>
