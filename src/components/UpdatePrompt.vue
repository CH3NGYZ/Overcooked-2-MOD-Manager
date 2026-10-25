<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
import { useMessage } from 'naive-ui'
import { NButton, NModal } from 'naive-ui'
import { api, watchAppUpdatePhase } from '../services/tauri'
import type { AppUpdateInfo } from '../types'

const props = defineProps<{ show: boolean; update: AppUpdateInfo | null }>()
const emit = defineEmits<{ 'update:show': [value: boolean] }>()

const message = useMessage()
const installing = ref(false)
const phaseText = ref('')

let unlisten: (() => void) | null = null
void watchAppUpdatePhase((event) => {
  if (event.message) phaseText.value = event.message
}).then((un) => { unlisten = un })
onUnmounted(() => { unlisten?.() })

const bodyText = computed(() => props.update?.body?.trim() || '本次更新没有提供说明。')

function close() {
  emit('update:show', false)
}

async function install() {
  installing.value = true
  phaseText.value = '准备下载更新…'
  try {
    const result = await api.installAppUpdate()
    if (result.ok) {
      message.success(result.message || '更新安装已开始，应用将自动退出')
      phaseText.value = ''
      close()
    } else {
      message.error(result.message)
      phaseText.value = ''
    }
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
    phaseText.value = ''
  } finally {
    installing.value = false
  }
}
</script>

<template>
  <n-modal :show="show" :mask-closable="false" :escable="false" @update:show="close">
    <div class="update-card">
      <h2 class="title">
        发现新版本<span v-if="update">: v{{ update.version }}</span>
      </h2>
      <div v-if="update" class="version-line muted">
        当前 v{{ update.current_version }} → 新版 v{{ update.version }}
        <span v-if="update.provider">（{{ update.provider === 'gitee' ? 'Gitee 渠道' : 'GitHub 渠道' }}）</span>
      </div>
      <div class="body-label muted">更新内容：</div>
      <div class="body">{{ bodyText }}</div>
      <div v-if="phaseText" class="phase">{{ phaseText }}</div>
      <div class="actions">
        <n-button quaternary :disabled="installing" @click="close">取消</n-button>
        <n-button type="primary" :loading="installing" @click="install">更新</n-button>
      </div>
    </div>
  </n-modal>
</template>

<style scoped>
.update-card {
  width: min(520px, calc(100vw - 80px));
  border-radius: 16px;
  background: #1e2025;
  border: 1px solid #2c3540;
  box-shadow: 0 24px 70px rgba(0, 0, 0, .55);
  padding: 24px 26px 18px;
}
.title {
  margin: 0 0 6px;
  font-size: 21px;
  font-weight: 700;
}
.version-line { margin-bottom: 12px; }
.body-label { margin-bottom: 6px; }
.body {
  max-height: 180px;
  overflow: auto;
  white-space: pre-wrap;
  padding: 10px 12px;
  border: 1px solid #273341;
  border-radius: 8px;
  background: #181b20;
  font-size: 13px;
  line-height: 1.6;
  color: #c3cdd8;
}
.phase {
  margin-top: 10px;
  color: #3cb89b;
  font-size: 12.5px;
}
.actions {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  margin-top: 16px;
}
</style>
