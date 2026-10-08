<script setup lang="ts">
import { onUnmounted, ref, watch } from 'vue'
import { ArrowLeft, ArrowRight, Check, CheckCircle2, Copy, Download, Minus, Play, Plus, RotateCcw, Save, Search, SkipForward, Trash2, Upload, X } from 'lucide-vue-next'
import { t } from '../i18n'
import tutorialPreviousImage from '../assets/tutorial/tutorial_1.jpg'
import tutorialNextImage from '../assets/tutorial/tutorial_2.jpg'
import tutorialZoomImage from '../assets/tutorial/tutorial_3.jpg'
import { tutorialLocaleOptions, tutorialScenarioOptions, type View } from '../tutorial/steps'
import type { useTutorialController } from '../tutorial/useTutorialController'

const props = defineProps<{
  controller: ReturnType<typeof useTutorialController>
  navItems: Array<{ id: View; label: string }>
}>()

const c = props.controller
const tutorialCard = ref<HTMLElement | null>(null)
const introductionGuides = [
  { image: tutorialPreviousImage, title: '上一步', description: '↑／← 或滚轮向上' },
  { image: tutorialNextImage, title: '下一步', description: '↓／→ 或滚轮向下' },
  { image: tutorialZoomImage, title: '界面缩放', description: 'Ctrl + 滚轮调整界面大小' },
]

watch(tutorialCard, (element) => {
  c.cardElement.value = element
}, { immediate: true })

onUnmounted(() => {
  c.cardElement.value = null
})
</script>

<template>
  <Teleport to="body">
    <div v-if="c.visible.value && (c.introductionVisible.value || c.currentStep.value)" :class="['tutorial-overlay', { 'tutorial-introduction-overlay': c.introductionVisible.value }]" @click="c.overlayClick">
      <div v-if="!c.introductionVisible.value" class="tutorial-spotlight" :style="c.spotlightStyle.value" aria-hidden="true"></div>
      <div ref="tutorialCard" class="tutorial-card" :style="c.cardStyle.value" role="dialog" aria-modal="true" aria-labelledby="tutorial-heading">
        <div class="tutorial-card-head">
          <span class="tutorial-progress" :title="t('方向键或滚轮切换步骤')">{{ c.introductionVisible.value ? t('操作说明') : c.progress.value }}</span>
          <button class="icon-button" :title="t('跳过教程')" :aria-label="t('跳过教程')" :disabled="c.saving.value" @click="c.requestFinish">
            <SkipForward :size="16" />
          </button>
        </div>
        <template v-if="c.introductionVisible.value">
          <h2 id="tutorial-heading">{{ t('教程操作') }}</h2>
          <div class="tutorial-introduction-grid">
            <figure v-for="guide in introductionGuides" :key="guide.title" class="tutorial-introduction-item">
              <img :src="guide.image" :alt="t(guide.title)" />
              <figcaption>
                <b>{{ t(guide.title) }}</b>
                <p>{{ t(guide.description) }}</p>
              </figcaption>
            </figure>
          </div>
          <div class="tutorial-actions">
            <button class="primary-button" :disabled="c.navigationBusy.value" @click="c.nextStep"><Play :size="16" />{{ t('开始教程') }}</button>
          </div>
        </template>
        <template v-else-if="c.currentStep.value">
          <h2 id="tutorial-heading">{{ c.stepText(c.currentStep.value, 'title') }}</h2>
          <p>{{ c.stepText(c.currentStep.value, 'description') }}</p>
          <div v-if="c.zoomStepActive.value" class="tutorial-zoom-controls" role="group" :aria-label="t('界面缩放')">
            <button class="secondary-button tutorial-icon-button" :title="t('缩小界面')" :aria-label="t('缩小界面')" :disabled="!c.canZoomOut.value" @click="c.adjustZoom(-1)"><Minus :size="18" /></button>
            <output class="tutorial-zoom-value" aria-live="polite">{{ c.zoomPercent.value }}%</output>
            <button class="secondary-button tutorial-icon-button" :title="t('放大界面')" :aria-label="t('放大界面')" :disabled="!c.canZoomIn.value" @click="c.adjustZoom(1)"><Plus :size="18" /></button>
          </div>
          <div v-if="c.currentStep.value.scenario === 'overview'" class="tutorial-jump-actions">
            <button class="ghost-button" @click="c.jumpToView('framework')">{{ t('框架管理') }}</button>
            <button class="ghost-button" @click="c.jumpToView('repositories')">{{ t('MOD安装') }}</button>
            <button class="ghost-button" @click="c.jumpToView('installed')">{{ t('MOD管理') }}</button>
          </div>
          <div class="tutorial-actions">
            <button v-if="c.stepIndex.value > 0 || !c.manualPreview.value" class="secondary-button tutorial-icon-button" :title="t('上一步')" :aria-label="t('上一步')" :disabled="c.navigationBusy.value" @click="c.previousStep"><ArrowLeft :size="18" /></button>
            <button class="primary-button tutorial-icon-button" :title="c.stepIndex.value === c.pendingSteps.value.length - 1 ? t('完成教程') : t('下一步')" :aria-label="c.stepIndex.value === c.pendingSteps.value.length - 1 ? t('完成教程') : t('下一步')" :disabled="c.navigationBusy.value" @click="c.nextStep"><Check v-if="c.stepIndex.value === c.pendingSteps.value.length - 1" :size="18" /><ArrowRight v-else :size="18" /></button>
          </div>
        </template>
      </div>
    </div>
  </Teleport>

  <Teleport to="body">
    <div v-if="c.exitConfirmVisible.value" class="modal-backdrop tutorial-confirm-backdrop" @click.self="c.exitConfirmVisible.value = false">
      <div class="modal tutorial-confirm-modal">
        <div class="modal-head">
          <div>
            <h3>{{ t('关闭教程') }}</h3>
            <p class="muted-text">{{ t('教程已关闭，之后可以从赞助渠道页面重新观看教程。') }}</p>
          </div>
          <button class="icon-button" :title="t('关闭')" @click="c.exitConfirmVisible.value = false"><X :size="17" /></button>
        </div>
        <div class="modal-actions">
          <button class="secondary-button" @click="c.exitConfirmVisible.value = false">{{ t('继续教程') }}</button>
          <button class="primary-button" @click="c.confirmFinish">{{ t('关闭教程') }}</button>
        </div>
      </div>
    </div>
    <div v-if="c.replayMenuVisible.value" class="modal-backdrop tutorial-confirm-backdrop" @click.self="c.replayMenuVisible.value = false">
      <div class="modal tutorial-confirm-modal">
        <div class="modal-head">
          <div>
            <h3>{{ t('重新观看教程') }}</h3>
            <p class="muted-text">{{ t('请选择重新观看教程或进入教程配置。') }}</p>
          </div>
          <button class="icon-button" :title="t('关闭')" @click="c.replayMenuVisible.value = false"><X :size="17" /></button>
        </div>
        <div class="modal-actions">
          <button class="primary-button" @click="c.replay">{{ t('重新观看教程') }}</button>
          <button class="secondary-button" @click="c.openEditor">{{ t('配置教程') }}</button>
        </div>
      </div>
    </div>
  </Teleport>

  <div v-if="c.pickerActive.value" class="tutorial-picker-banner">
    <span>{{ t('选择教程目标') }}<small v-if="c.pickerHoverSelector.value">{{ c.pickerHoverSelector.value }}</small></span>
    <button class="ghost-button" @click="c.stopPicker()">{{ t('取消选择') }}</button>
  </div>
  <div v-if="c.pickerActive.value && c.pickerHoverRect.value.width > 0" class="tutorial-picker-highlight"
    :style="c.pickerHighlightStyle.value" aria-hidden="true"></div>

  <div v-if="c.editorVisible.value" class="modal-backdrop" @click.self="c.editorVisible.value = false">
    <div class="modal tutorial-editor-modal">
      <div class="modal-head">
        <div>
          <h3>{{ t('编辑首次启动教程') }}</h3>
          <p class="muted-text">{{ t('使用 CSS 层级选择器定位需要说明的界面区域。') }}</p>
        </div>
        <button class="icon-button" :title="t('关闭')" :disabled="c.saving.value" @click="c.editorVisible.value = false"><X :size="17" /></button>
      </div>
      <div class="tutorial-editor-list">
        <div v-for="(step, index) in c.editorSteps.value" :key="`${step.id}-${index}`" class="tutorial-editor-step">
          <div class="tutorial-editor-step-head">
            <b>{{ t('教程步骤') }} {{ index + 1 }}</b>
            <button class="icon-button small danger-icon" :title="t('删除步骤')" :disabled="c.saving.value" @click="c.removeStep(index)"><Trash2 :size="14" /></button>
          </div>
          <div class="form-grid">
            <label><span>{{ t('步骤 ID') }}</span><input v-model="step.id" /></label>
            <label>
              <span>{{ t('进入页面') }}</span>
              <select v-model="step.view">
                <option value="">{{ t('不改变当前页面') }}</option>
                <option v-for="item in navItems" :key="item.id" :value="item.id">{{ t(item.label) }}</option>
              </select>
            </label>
            <label>
              <span>{{ t('教程展示场景') }}</span>
              <select v-model="step.scenario">
                <option v-for="item in tutorialScenarioOptions" :key="item.value" :value="item.value">{{ t(item.label) }}</option>
              </select>
            </label>
            <label class="wide"><span>{{ t('CSS 选择器') }}</span><input v-model="step.selector" placeholder=".app-shell > .sidebar > .nav-list > .nav-item" /></label>
            <div class="tutorial-editor-selector-actions wide">
              <button class="secondary-button" :disabled="c.saving.value" @click="c.startPicker(index)"><Search :size="15" />{{ t('从上面选择的进入页面里面去选择') }}</button>
            </div>
            <div class="tutorial-language-fields wide">
              <span class="tutorial-language-label">{{ t('步骤标题') }}</span>
              <div class="tutorial-language-grid">
                <label v-for="language in tutorialLocaleOptions" :key="`title-${language.code}`">
                  <span>{{ language.label }}</span><input v-model="step.title_i18n[language.code]" />
                </label>
              </div>
            </div>
            <div class="tutorial-language-fields wide">
              <span class="tutorial-language-label">{{ t('步骤说明') }}</span>
              <div class="tutorial-language-grid">
                <label v-for="language in tutorialLocaleOptions" :key="`description-${language.code}`">
                  <span>{{ language.label }}</span><textarea v-model="step.description_i18n[language.code]" rows="3" /></label>
              </div>
            </div>
          </div>
        </div>
      </div>
      <button class="secondary-button tutorial-editor-add" :disabled="c.saving.value" @click="c.addStep"><Plus :size="15" />{{ t('添加步骤') }}</button>
      <div class="modal-actions">
        <button class="secondary-button" :disabled="c.saving.value" @click="c.preview"><Play :size="15" />{{ t('预览教程') }}</button>
        <button class="secondary-button" :disabled="c.saving.value" @click="c.exportConfig"><Upload :size="15" />{{ t('导出教程格式') }}</button>
        <button class="secondary-button" :disabled="c.saving.value" @click="c.openImport"><Download :size="15" />{{ t('粘贴教程格式') }}</button>
        <button class="ghost-button" :disabled="c.saving.value" @click="c.restoreBuiltIn"><RotateCcw :size="15" />{{ t('恢复内置教程') }}</button>
        <button class="secondary-button" :disabled="c.saving.value" @click="c.editorVisible.value = false">{{ t('取消') }}</button>
        <button class="primary-button" :disabled="c.saving.value" @click="c.saveEditor"><Save :size="15" />{{ t('保存并重新展示') }}</button>
      </div>
    </div>
  </div>

  <div v-if="c.importVisible.value" class="modal-backdrop" @click.self="c.importVisible.value = false">
    <div class="modal tutorial-import-modal">
      <div class="modal-head">
        <div>
          <h3>{{ t('粘贴教程格式') }}</h3>
          <p class="muted-text">{{ t('教程格式说明') }}</p>
        </div>
        <button class="icon-button" :title="t('关闭')" @click="c.importVisible.value = false"><X :size="17" /></button>
      </div>
      <textarea v-model="c.importText.value" class="tutorial-import-input" :placeholder="t('粘贴教程 TOML 内容')"></textarea>
      <div class="tutorial-import-actions">
        <button class="secondary-button" :disabled="c.importing.value" @click="c.copyFormat"><Copy :size="15" />{{ t('复制格式模板') }}</button>
        <button class="primary-button" :disabled="c.importing.value || !c.importText.value.trim()" @click="c.previewImport"><Search :size="15" />{{ t('解析并预览') }}</button>
      </div>
      <pre class="tutorial-format-template">{{ c.formatTemplate }}</pre>
      <p v-if="c.importError.value" class="tutorial-import-error">{{ c.importError.value }}</p>
      <div v-if="c.importPreview.value" class="tutorial-import-preview"><CheckCircle2 :size="16" />{{ t('格式有效') }} · {{ c.importPreview.value.length }} {{ t('教程步骤') }}</div>
      <div class="modal-actions">
        <button class="secondary-button" @click="c.importVisible.value = false">{{ t('取消') }}</button>
        <button class="primary-button" :disabled="!c.importPreview.value" @click="c.applyImport"><Check :size="15" />{{ t('导入到编辑器') }}</button>
      </div>
    </div>
  </div>

  <div v-if="c.exportVisible.value" class="modal-backdrop" @click.self="c.exportVisible.value = false">
    <div class="modal tutorial-export-modal">
      <div class="modal-head">
        <div>
          <h3>{{ t('导出教程格式') }}</h3>
          <p class="muted-text">{{ t('将下面的 TOML 粘贴到教程格式框中，或保存到 MOD 的内置教程文件。') }}</p>
        </div>
        <button class="icon-button" :title="t('关闭')" @click="c.exportVisible.value = false"><X :size="17" /></button>
      </div>
      <textarea class="tutorial-export-code" readonly :value="c.exportCode.value" @focus="c.selectExport" />
      <div class="modal-actions">
        <button class="secondary-button" @click="c.copyExport"><Copy :size="15" />{{ t('复制教程格式') }}</button>
        <button class="primary-button" @click="c.exportVisible.value = false">{{ t('关闭') }}</button>
      </div>
    </div>
  </div>
</template>
