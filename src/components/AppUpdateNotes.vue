<script setup lang="ts">
import { computed } from 'vue'
import { updateNotesForLocale } from '../appUpdateNotes'
import { locale, t } from '../i18n'
import type { AppUpdateInfo } from '../types'
import MarkdownBody from './MarkdownBody.vue'

const props = defineProps<{ update: AppUpdateInfo | null }>()
const notes = computed(() => updateNotesForLocale(props.update, locale.value))
</script>

<template>
  <div class="update-notes">
    <section v-for="entry in notes" :key="entry.version ?? 'target'">
      <h3 v-if="entry.version">v{{ entry.version }}</h3>
      <MarkdownBody v-if="entry.notes" :source="entry.notes" />
      <p v-else>{{ t('dialogs.noReleaseNotesWereProvided') }}</p>
    </section>
  </div>
</template>

<style scoped>
.update-notes {
  padding: 10px 12px; border: 1px solid var(--oc2-divider); border-radius: 8px;
  background: var(--oc2-sunken); font-size: 13px; line-height: 1.6; color: var(--oc2-text-2);
}
.update-notes section + section { margin-top: 14px; }
.update-notes h3 { margin: 0 0 4px; font-size: 15px; }
.update-notes p { margin: 0; }
</style>
