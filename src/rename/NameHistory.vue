<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, useTemplateRef } from 'vue'
import { useDialog } from 'naive-ui'
import { ChevronForwardOutline, CloseOutline, TrashOutline } from '@vicons/ionicons5'
import HeaderIconAction from '../components/HeaderIconAction.vue'
import { t } from '../i18n'
import { styleName } from '../colorfulName'
import { NAME_HISTORY_LIMIT, nameHistory, type NameDraft } from './history'
import NameCharacters from './NameCharacters.vue'

defineEmits<{ restore: [draft: NameDraft] }>()
const dialog = useDialog()
const heading = useTemplateRef('heading')
const list = useTemplateRef('list')
const rows = computed(() => nameHistory.entries.value.map(entry => ({
  ...entry, chars: styleName(entry.name, entry.colors, entry.sizes),
})))
let confirmation: ReturnType<typeof dialog.warning> | undefined
onBeforeUnmount(() => confirmation?.destroy())

async function remove(key: string, index: number) {
  nameHistory.remove(key)
  await nextTick()
  const buttons = list.value?.querySelectorAll<HTMLButtonElement>('button.history-preview')
  const next = buttons?.[index] ?? buttons?.[index - 1]
  ;(next ?? heading.value)?.focus()
}

function confirmClear() {
  if (confirmation) return
  confirmation = dialog.warning({
    title: t('rename.clearNameHistory'),
    content: t('rename.clearNameHistoryConfirm'),
    positiveText: t('rename.clearNameHistory'),
    negativeText: t('common.action.cancel'),
    maskClosable: false,
    onPositiveClick: nameHistory.clear,
    onAfterLeave: () => {
      confirmation = undefined
      if (!rows.value.length) heading.value?.focus()
    },
  })
}
</script>

<template>
  <section class="history-panel" aria-labelledby="rename-history-heading" data-rename-history>
    <header class="history-head">
      <h2 id="rename-history-heading" ref="heading" tabindex="-1">{{ t('rename.nameHistory') }}</h2>
      <span v-if="rows.length" class="muted history-count">{{ rows.length }} / {{ NAME_HISTORY_LIMIT }}</span>
      <HeaderIconAction v-if="rows.length" size="tiny" quaternary circle
        :label="t('rename.clearNameHistory')" @click="confirmClear">
        <TrashOutline />
      </HeaderIconAction>
    </header>
    <p v-if="!rows.length" class="muted history-empty">{{ t('rename.nameHistoryEmpty') }}</p>
    <ul v-else ref="list" class="history-list">
      <li v-for="(entry, index) in rows" :key="entry.key" class="history-row">
        <button type="button" class="name-preview-surface history-preview"
          :aria-label="t('rename.reuseHistoryName', { name: entry.name })" @click="$emit('restore', entry)">
          <span class="history-name"><NameCharacters :chars="entry.chars" /></span>
          <ChevronForwardOutline class="history-chevron" aria-hidden="true" />
        </button>
        <HeaderIconAction size="tiny" quaternary circle :label="t('rename.removeHistoryName')"
          :description="entry.name" @click="remove(entry.key, index)">
          <CloseOutline />
        </HeaderIconAction>
      </li>
    </ul>
  </section>
</template>

<style scoped>
.history-panel {
  min-width: 0;
  padding: 14px 16px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
}
.history-head { display: flex; align-items: center; gap: 8px; }
.history-head h2 { margin: 0; font-size: 13.5px; color: var(--oc2-text-1); }
.history-count { margin-inline-end: auto; font-size: 11.5px; white-space: nowrap; font-variant-numeric: tabular-nums; }
.history-empty { margin: 6px 0 0; }
.history-list { list-style: none; margin: 8px 0 0; padding: 0; display: grid; gap: 6px; }
.history-row { display: grid; grid-template-columns: minmax(0, 1fr) auto; gap: 6px; align-items: center; }
.history-preview {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  width: 100%;
  min-height: 34px;
  padding: 3px 9px;
  font: inherit;
  text-align: start;
  border: 1px solid var(--oc2-row-line);
  border-radius: 8px;
  cursor: pointer;
}
.history-preview:hover { border-color: var(--oc2-accent); }
.history-preview:focus-visible, .history-head h2:focus-visible { outline: 2px solid var(--oc2-accent); outline-offset: 2px; }
.history-name { min-width: 0; line-height: 1.2; }
.history-chevron { width: 16px; height: 16px; flex: 0 0 auto; color: #546275; }
</style>
