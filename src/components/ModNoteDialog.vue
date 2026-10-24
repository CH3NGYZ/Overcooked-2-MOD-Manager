<script setup lang="ts">
import { Save, X } from 'lucide-vue-next'
import { t } from '../i18n'
import type { ModOperations } from '../composables/useModOperations'

defineProps<{ ops: ModOperations; busy?: string }>()
const emit = defineEmits<{ close: [] }>()
</script>

<template>
  <div v-if="ops.editingMod.value" class="modal-backdrop" @click.self="emit('close')">
    <div class="modal">
      <div class="modal-head">
        <div>
          <h3>{{ ops.editingMod.value.name }}</h3>
        </div><button class="icon-button" @click="emit('close')">
          <X :size="17" />
        </button>
      </div>
      <p class="muted-text">{{ t('产品名称：') }}{{ ops.editingMod.value.product_name || t('未知') }} · {{ ops.displayModPath(ops.editingMod.value) }}</p>
      <textarea v-model="ops.modNote.value" rows="6" :placeholder="t('例如：这是街机模组，包含房主工具、房间控制和快捷设置功能。')" />
      <div class="modal-actions"><button class="secondary-button" @click="emit('close')">{{ t('取消')
          }}</button><button class="primary-button" :disabled="!!busy" @click="ops.saveModNote()">
            <Save :size="16" />{{ t('保存备注') }}
          </button></div>
    </div>
  </div>
</template>
