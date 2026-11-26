<script setup lang="ts">
import { computed, shallowRef, useTemplateRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import { MegaphoneOutline } from '@vicons/ionicons5'
import { announcementSnapshot, announcementsUnread, markAnnouncementsRead } from '../announcements/state'
import protocol from '../announcements/protocol.json' with { type: 'json' }
import { locale, t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import TitleBarStatusPopover from './TitleBarStatusPopover.vue'

const props = defineProps<{ disabled: boolean }>()
const notify = useNotify()
const open = shallowRef(false)
const list = useTemplateRef<HTMLOListElement>('list')
const label = computed(() => announcementsUnread.value ? t('announcements.unread') : t('announcements.title'))
const dateFormatter = computed(() => new Intl.DateTimeFormat(locale.value, { dateStyle: 'medium', timeStyle: 'short' }))
const displayedNotices = computed(() => announcementSnapshot.value?.announcements.map(notice => {
  const translated = notice.translations[locale.value]
  return { id: notice.id, publishedAt: notice.published_at, language: translated ? locale.value : notice.language, text: translated ?? notice }
}) ?? [])

watch(() => props.disabled, disabled => { if (disabled) open.value = false })
watch([open, list, announcementSnapshot], ([visible, rendered]) => {
  if (visible && !props.disabled && rendered?.isConnected) markAnnouncementsRead()
}, { flush: 'post' })

async function openAnnouncementPage() {
  try { await api.openExternalPage(protocol.origin) }
  catch (reason) { notify.error(backendMessageOf(reason)) }
}
</script>

<template>
  <TitleBarStatusPopover v-model:open="open" :label="label" :title="label"
    :tone="announcementsUnread ? 'warning' : undefined" :disabled="props.disabled" data-testid="announcement-status">
    <template #icon><MegaphoneOutline width="17" height="17" aria-hidden="true" /></template>
    <strong>{{ t('announcements.title') }}</strong>
    <p v-if="announcementSnapshot === undefined" role="status">{{ t('announcements.loading') }}</p>
    <template v-else-if="announcementSnapshot === null">
      <p role="status">{{ t('announcements.unavailable') }}</p>
      <NButton size="small" secondary class="announcement-open" data-testid="announcement-open" @click="openAnnouncementPage">
        {{ t('announcements.open') }}
      </NButton>
    </template>
    <p v-else-if="!displayedNotices.length">{{ t('announcements.empty') }}</p>
    <ol v-else ref="list" class="announcement-list" :aria-label="t('announcements.title')" tabindex="0" data-testid="announcement-list">
      <li v-for="notice in displayedNotices" :key="notice.id">
        <h3 :lang="notice.language">{{ notice.text.title }}</h3>
        <time :datetime="notice.publishedAt">{{ dateFormatter.format(new Date(notice.publishedAt)) }}</time>
        <p class="announcement-body" :lang="notice.language">{{ notice.text.body }}</p>
      </li>
    </ol>
  </TitleBarStatusPopover>
</template>

<style scoped>
p { margin: 0; }
.announcement-open { align-self: flex-end; }
.announcement-list {
  max-height: min(230px, 47vh); overflow: auto; overscroll-behavior: contain; scrollbar-gutter: stable;
  padding: 0 6px 0 0; margin: 0; list-style: none; overflow-wrap: anywhere;
}
.announcement-list:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
li { border-top: 1px solid var(--oc2-rail-line); padding: 8px 0; }
li:last-child { padding-bottom: 0; }
h3 { margin: 0; font-size: 14px; color: var(--oc2-text-1); }
time { color: var(--oc2-text-3); font-size: 11px; }
.announcement-body { margin-top: 4px; white-space: pre-wrap; }
</style>
