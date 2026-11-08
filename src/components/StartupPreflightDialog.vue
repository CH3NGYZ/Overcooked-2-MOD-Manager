<script setup lang="ts">
import { computed } from 'vue'
import { NButton, NSpin } from 'naive-ui'
import AppModal from './AppModal.vue'
import type { PreflightStage, createStartupPreflight } from '../startupPreflight'
import { appUpdateState } from '../appUpdateState'
import { t } from '../i18n'

const { state, gate } = defineProps<{
  state: { stage: PreflightStage; failure: string }
  gate: ReturnType<typeof createStartupPreflight>
}>()
const checking = computed(() => state.stage === 'checking' || state.stage === 'closing')
const visible = computed(() => appUpdateState.stage === 'idle' &&
  ['checking', 'failed', 'closing'].includes(state.stage))
const title = computed(() => t(checking.value ? 'startup.checking' : 'startup.failed'))
const failureKey = computed(() => `startup.failure.${state.failure || 'network'}`)
function onVisible(show: boolean) {
  if (!show) void gate.close()
}
</script>

<template>
  <AppModal :show="visible" :title="title" max-width="600px" :mask-closable="false"
    @update:show="onVisible">
    <div class="startup-preflight" aria-live="polite" :aria-busy="checking">
      <NSpin v-if="checking" size="small" />
      <template v-else>
        <p role="status">{{ t(failureKey) }}</p>
        <p>{{ t('startup.mainland') }}</p>
        <p>{{ t('startup.international') }}</p>
      </template>
    </div>
    <template #footer>
      <NButton v-if="state.stage === 'failed'" @click="gate.retry()">{{ t('common.action.retry') }}</NButton>
      <NButton :disabled="state.stage === 'closing'" @click="gate.close()">
        {{ t('startup.continue') }}
      </NButton>
    </template>
  </AppModal>
</template>

<style scoped>
.startup-preflight { min-height: 40px; }
.startup-preflight p { margin: 0 0 12px; line-height: 1.65; }
</style>
