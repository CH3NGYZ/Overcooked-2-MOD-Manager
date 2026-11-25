<script setup lang="ts">
import { ref } from 'vue'
import { NButton, NRadio, NRadioGroup } from 'naive-ui'
import { locale, localeOptions, setLocale, t } from '../i18n'
import AppModal from './AppModal.vue'

const emit = defineEmits<{ confirmed: [] }>()
const show = ref(true)

function confirm() {
  if (!show.value) return
  setLocale(locale.value)
  show.value = false
}
</script>

<template>
  <AppModal :show="show" :title="t('startup.language.title')" :closable="false"
    max-width="min(540px, 90vw)" max-height="90%" :aria-label="t('startup.language.title')"
    data-testid="startup-language" @after-leave="emit('confirmed')">
    <NRadioGroup v-model:value="locale" name="startup-language" class="language-grid"
      :aria-label="t('startup.language.title')">
      <NRadio v-for="option in localeOptions" :key="option.value" :value="option.value"
        :lang="option.value" :class="{ selected: locale === option.value }">
        {{ option.label }}
      </NRadio>
    </NRadioGroup>
    <p class="language-hint">{{ t('startup.language.hint') }}</p>
    <template #footer>
      <NButton type="primary" data-testid="startup-language-continue" @click="confirm">
        {{ t('startup.language.continue') }}
      </NButton>
    </template>
  </AppModal>
</template>

<style scoped>
.language-grid {
  width: min(480px, 100%);
  display: grid;
  grid-template-columns: repeat(3, minmax(0, 1fr));
  gap: 8px;
}
.language-grid :deep(.n-radio) {
  padding: 8px;
  min-width: 0;
  border: 1px solid var(--oc2-divider);
  border-radius: 8px;
  background: var(--oc2-row);
  overflow-wrap: anywhere;
}
.language-grid :deep(.selected) {
  border-color: var(--oc2-accent);
  background: var(--oc2-row-hover);
}
.language-grid :deep(.n-radio:focus-within) { outline: 2px solid var(--oc2-accent); outline-offset: -2px; }
.language-hint { margin: 12px 0 0; color: var(--oc2-text-2); }
@media (max-width: 470px) {
  .language-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
