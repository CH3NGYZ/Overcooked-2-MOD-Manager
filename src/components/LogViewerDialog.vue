<script setup lang="ts">
import { nextTick, ref, watch } from 'vue'
import { RefreshCw, X } from 'lucide-vue-next'
import { t } from '../i18n'
import type { LogViewerKind } from '../composables/useLogViewer'

const props = defineProps<{ open: boolean; kind: LogViewerKind; log: { path: string; content: string }; busy?: boolean }>()
const emit = defineEmits<{ close: []; refresh: [] }>()
const output = ref<HTMLElement | null>(null)

watch(() => props.log.content, async () => {
  await nextTick()
  if (output.value) output.value.scrollTop = output.value.scrollHeight
})
</script>

<template>
  <div v-if="open" class="modal-backdrop" @click.self="emit('close')">
    <div class="modal rust-log-modal">
      <div class="modal-head">
        <div>
          <h3>{{ t(kind === 'rust' ? 'Rust 运行日志' : kind === 'frontend' ? '前端运行日志' : '崩溃日志') }}</h3><small class="rust-log-path">{{ log.path }}</small>
        </div><button class="icon-button" @click="emit('close')">
          <X :size="17" />
        </button>
      </div>
      <pre ref="output" class="rust-log-output">{{ log.content || t('暂无 Rust 日志') }}</pre>
      <div class="modal-actions"><button class="secondary-button" @click="emit('refresh')">
          <RefreshCw :size="15" />{{ t('立即刷新') }}
        </button><button class="primary-button" @click="emit('close')">{{ t('关闭') }}</button></div>
    </div>
  </div>
</template>
