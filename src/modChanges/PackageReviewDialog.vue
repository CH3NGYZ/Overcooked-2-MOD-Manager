<script setup lang="ts">
import { computed, shallowRef, watch } from 'vue'
import { NButton, NTag } from 'naive-ui'
import AppModal from '../components/AppModal.vue'
import MarkdownBody from '../components/MarkdownBody.vue'
import { locale, t } from '../i18n'
import { packageReleaseBody } from '../packageReleaseNotes'
import { installNoteKey, type InstallNote } from './planNotes'
import type { PackageReviewRelease } from './store'

const props = withDefaults(defineProps<{
  notes: readonly InstallNote[]
  release?: PackageReviewRelease
  show?: boolean
  reviewKey?: string
  preflight?: boolean
  applyOnRead?: boolean
}>(), { show: true })
const emit = defineEmits<{ continue: [notes: readonly InstallNote[]]; cancel: []; afterLeave: [] }>()
const READ_SECONDS = 5
const remaining = shallowRef(0)
const target = computed(() => props.release?.releases.at(-1))
const releaseNotes = computed(() => props.notes.length ? [] : (props.release?.releases ?? []).map(entry => ({
  ...entry, body: packageReleaseBody(entry.body, locale.value),
})))

watch(() => JSON.stringify([props.show, props.reviewKey, props.applyOnRead, props.notes.map(installNoteKey)]), (_, __, onCleanup) => {
  if (!props.show || !props.notes.length) { remaining.value = 0; return }
  remaining.value = READ_SECONDS
  const deadline = performance.now() + READ_SECONDS * 1000
  const timer = setInterval(() => {
    remaining.value = Math.max(0, Math.ceil((deadline - performance.now()) / 1000))
    if (!remaining.value) clearInterval(timer)
  }, 100)
  onCleanup(() => clearInterval(timer))
}, { immediate: true, flush: 'sync' })

function continueReading() {
  if (props.show && remaining.value === 0) emit('continue', props.notes)
}
</script>

<template>
  <AppModal :show="show" :title="t(notes.length ? 'dialogs.readModNoteFirst' : 'dialogs.packageReleaseNotes')"
    max-width="min(640px, 94%)" max-height="90%" :mask-closable="false" card-class="package-review-modal"
    @update:show="(visible: boolean) => { if (!visible) emit('cancel') }" @after-leave="emit('afterLeave')">
    <div class="package-review" :data-package-preflight="preflight ? '' : undefined">
      <div v-if="release && target" class="package-review-heading">
        <h3>{{ release.name }}</h3>
        <div class="package-review-version">
          <span v-if="release.installedTag && release.installedTag !== target.tag" class="muted">{{ release.installedTag }} →</span>
          <strong>{{ target.tag }}</strong>
          <NTag v-if="target.prerelease" size="small" type="warning" data-package-preflight-kind="prerelease">
            {{ t('modsOnline.preRelease') }}
          </NTag>
        </div>
      </div>
      <article v-for="article in notes" :key="article.id" class="package-review-entry" data-package-preflight-kind="install-note">
        <h3 v-if="!release">{{ article.name }}</h3>
        <MarkdownBody :source="article.text" />
      </article>
      <section v-if="releaseNotes.length" data-package-preflight-kind="release-notes">
        <article v-for="entry in releaseNotes" :key="entry.tag" class="package-review-entry" :data-package-release-tag="entry.tag">
          <h4>{{ entry.tag }} <small v-if="entry.prerelease && entry.tag !== target?.tag">{{ t('modsOnline.preRelease') }}</small></h4>
          <MarkdownBody v-if="entry.body.trim()" :source="entry.body" />
          <p v-else class="muted">{{ t('dialogs.noReleaseNotesWereProvided') }}</p>
        </article>
      </section>
    </div>
    <template #footer>
      <div class="package-review-actions">
        <NButton data-package-preflight-action="cancel" @click="emit('cancel')">{{ t('common.action.cancel') }}</NButton>
        <NButton type="primary" :disabled="remaining > 0" data-package-preflight-action="continue" @click="continueReading">
          {{ t(applyOnRead ? 'modChanges.readAndApply' : 'dialogs.continueAfterReading') }}{{ remaining ? ` (${remaining})` : '' }}
        </NButton>
      </div>
    </template>
  </AppModal>
</template>

<style scoped>
.package-review { min-width: 0; overflow-wrap: anywhere; }
.package-review-heading { display: flex; flex-wrap: wrap; align-items: center; justify-content: space-between; gap: 6px 14px; margin-bottom: 12px; }
.package-review-version { display: flex; flex-wrap: wrap; align-items: center; gap: 5px; }
h3, h4, p { margin: 0; }
h4, .package-review-entry h3 { margin-bottom: 6px; }
h4 small { margin-inline-start: 4px; color: var(--oc2-text-3); }
.package-review-entry { border-radius: 8px; background: var(--oc2-sunken); border: 1px solid var(--oc2-divider); padding: 10px 12px; }
.package-review-entry + .package-review-entry { margin-top: 8px; }
.package-review-actions { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
</style>
