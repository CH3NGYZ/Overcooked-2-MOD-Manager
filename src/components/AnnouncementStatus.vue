<script setup lang="ts">
import { computed, shallowRef, useTemplateRef, watch } from 'vue'
import { NButton } from 'naive-ui'
import { MegaphoneOutline } from '@vicons/ionicons5'
import { announcementSnapshot, announcementsUnread, loadAnnouncements, markAnnouncementsRead } from '../announcements/state'
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

watch(() => props.disabled, disabled => { if (disabled) open.value = false })
watch([open, list, announcementSnapshot], ([visible, rendered]) => {
  if (visible && !props.disabled && rendered?.isConnected) markAnnouncementsRead()
}, { flush: 'post' })

async function openAnnouncementPage() {
  try { await api.openExternalPage(protocol.origin) }
  catch (reason) { notify.error(backendMessageOf(reason)) }
}

function refresh() {
  if (announcementSnapshot.value !== undefined) void loadAnnouncements(locale.value, api.announcements, true)
}
</script>

<template>
  <TitleBarStatusPopover v-model:open="open" :label="label" :title="label" :heading="t('announcements.title')"
    :tone="announcementsUnread ? 'warning' : undefined" :disabled="props.disabled" data-testid="announcement-status">
    <template #icon><MegaphoneOutline width="17" height="17" aria-hidden="true" /></template>
    <template #heading-action>
      <NButton size="small" secondary :loading="announcementSnapshot === undefined"
        :aria-disabled="announcementSnapshot === undefined" data-testid="announcement-refresh" @click="refresh">
        {{ t('common.action.refetch') }}
      </NButton>
    </template>
    <p v-if="announcementSnapshot === undefined" role="status">{{ t('announcements.loading') }}</p>
    <template v-else-if="announcementSnapshot === null">
      <p role="status">{{ t('announcements.unavailable') }}</p>
      <NButton size="small" secondary class="announcement-open" data-testid="announcement-open" @click="openAnnouncementPage">
        {{ t('announcements.open') }}
      </NButton>
    </template>
    <p v-else-if="!announcementSnapshot.announcements.length">{{ t('announcements.empty') }}</p>
    <ol v-else ref="list" class="announcement-list" :aria-label="t('announcements.title')" tabindex="0" data-testid="announcement-list">
      <li v-for="notice in announcementSnapshot.announcements" :key="notice.id">
        <h3 :lang="notice.language">{{ notice.title }}</h3>
        <time :datetime="notice.published_at">{{ dateFormatter.format(new Date(notice.published_at)) }}</time>
        <p class="announcement-body" :lang="notice.language">{{ notice.body }}</p>
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
