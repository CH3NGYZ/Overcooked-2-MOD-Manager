<script setup lang="ts">
import { ChatbubblesOutline, LogoDiscord } from '@vicons/ionicons5'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import SettingsRow from './settings/SettingsRow.vue'

defineProps<{ compact?: boolean }>()

const channels = [
  { title: 'tools.arcadeModFeedbackGroup1', url: 'https://qm.qq.com/q/Uo7nhlbWiA', icon: ChatbubblesOutline },
  { title: 'tools.arcadeModFeedbackGroup2', url: 'https://qm.qq.com/q/7CpekB9FYc', icon: ChatbubblesOutline },
  { title: 'tools.modManagerFeedbackGroup', url: 'https://qm.qq.com/q/LN93oZTyUg', icon: ChatbubblesOutline },
  { title: 'tools.modManagerFeedbackDiscord', url: 'https://discord.gg/nPt3nbr3WG', icon: LogoDiscord },
]
const notify = useNotify()

function openChannel(url: string) {
  void api.openReleasePage(url).catch(reason => notify.error(backendMessageOf(reason)))
}
</script>

<template>
  <div v-if="compact" class="feedback-grid" role="group" :aria-label="t('tools.feedbackChannels')"
    data-testid="tutorial-feedback-channels">
    <button v-for="channel in channels" :key="channel.url" class="feedback-button" type="button"
      @click="openChannel(channel.url)">
      <component :is="channel.icon" width="16" height="16" aria-hidden="true" />
      <span>{{ t(channel.title) }}</span>
    </button>
  </div>
  <template v-else>
    <SettingsRow v-for="channel in channels" :key="channel.url" :icon="channel.icon" :title="t(channel.title)"
      :description="t(channel.icon === LogoDiscord ? 'tools.discordFeedbackDescription' : 'tools.qqFeedbackGroupDescription')"
      clickable role="link" tabindex="0" @click="openChannel(channel.url)"
      @keydown.enter.prevent="openChannel(channel.url)" @keydown.space.prevent="openChannel(channel.url)" />
  </template>
</template>

<style scoped>
.feedback-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px;
}
.feedback-button {
  display: flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
  min-height: 36px;
  padding: 5px 7px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 6px;
  background: var(--oc2-row);
  color: var(--oc2-text-1);
  font: inherit;
  font-size: 12px;
  line-height: 1.35;
  text-align: start;
  cursor: pointer;
}
.feedback-button:hover { background: var(--oc2-row-hover); border-color: var(--oc2-hover-line); }
.feedback-button:focus-visible { outline: 2px solid var(--oc2-accent-contrast); outline-offset: -2px; }
.feedback-button svg { flex: none; color: var(--oc2-accent-contrast); }
.feedback-button span { min-width: 0; overflow-wrap: anywhere; }
</style>
