<script setup lang="ts">
import { NButton } from 'naive-ui'
import AppModal from './AppModal.vue'
import { gameOperationNotice } from '../composables/useGameOperationGate'
import { appUpdateState } from '../appUpdateState'
import { t } from '../i18n'
</script>

<template>
  <AppModal :show="gameOperationNotice && appUpdateState.stage === 'idle'"
    :title="t('common.gameRunning')" max-width="480px"
    @update:show="(value: boolean) => { gameOperationNotice = value }">
    <p data-testid="game-operation-blocked">{{ t('game.operationBlocked') }}</p>
    <p>{{ t('game.modOperationBlocked') }}</p>
    <template #footer>
      <NButton type="primary" @click="gameOperationNotice = false">{{ t('common.action.ok') }}</NButton>
    </template>
  </AppModal>
</template>
