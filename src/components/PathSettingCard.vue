<script setup lang="ts">
import { ref, watch } from 'vue'
import { NButton, NCard, NInput, NTag } from 'naive-ui'
import { FolderOpen, RotateCcw, Save } from 'lucide-vue-next'
import { api } from '../services/tauri'

/**
 * 通用路径设置卡：游戏目录、未来的 DIYLevel 自定义地图框架路径等
 * 都用这一个组件。后端动作（打开/保存/恢复）由宿主页面提供，
 * 组件只负责草稿输入、目录浏览与三个动作的触发。
 */
const props = withDefaults(defineProps<{
  title: string
  path: string
  description?: string
  verified?: boolean | null
  saving?: boolean
  saveLabel?: string
  resetLabel?: string
}>(), {
  verified: null,
  saving: false,
  saveLabel: '保存并验证',
  resetLabel: '恢复默认',
})

const emit = defineEmits<{ open: []; save: [path: string]; reset: [] }>()

const draft = ref('')
watch(() => props.path, (value) => { draft.value = value }, { immediate: true })

async function browse() {
  const dir = await api.chooseGameDirectory()
  if (dir) draft.value = dir
}

function save() {
  const next = draft.value.trim()
  if (!next) return
  emit('save', next)
}
</script>

<template>
  <n-card :title="title" class="path-card">
    <template #header-extra>
      <n-tag v-if="verified === true" type="success" size="small" :bordered="false">已验证</n-tag>
      <n-tag v-else-if="verified === false" type="warning" size="small" :bordered="false">未验证</n-tag>
    </template>

    <div class="row">
      <n-input v-model:value="draft" clearable placeholder="选择或输入目录路径" />
      <n-button secondary @click="browse"><FolderOpen :size="16" />浏览…</n-button>
    </div>
    <div v-if="description || path" class="hint muted">
      {{ description }}
      <template v-if="path">当前：{{ path }}</template>
    </div>
    <div class="actions">
      <n-button type="primary" :loading="saving" :disabled="!draft.trim()" @click="save">
        <Save :size="16" />{{ saveLabel }}
      </n-button>
      <n-button secondary :disabled="saving" @click="emit('reset')">
        <RotateCcw :size="16" />{{ resetLabel }}
      </n-button>
      <n-button quaternary :disabled="saving" @click="emit('open')">
        <FolderOpen :size="16" />打开路径
      </n-button>
    </div>
  </n-card>
</template>

<style scoped>
.path-card { max-width: 860px; }
.row { display: flex; gap: 10px; }
.row .n-input { flex: 1; }
.hint { margin: 10px 0 14px; }
.actions { display: flex; gap: 10px; flex-wrap: wrap; }
</style>
