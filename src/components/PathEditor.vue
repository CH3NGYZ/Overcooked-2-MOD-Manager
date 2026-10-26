<script setup lang="ts">
import { t } from '../i18n'
import { ref, watch } from 'vue'
import { NButton, NInput } from 'naive-ui'
import { FolderOpenOutline, SaveOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'

/**
 * 路径编辑字段集：输入 + 浏览 + 确认保存。
 * 游戏目录与未来 DIYLevel 自定义地图框架路径的二级菜单共用。
 */
const props = withDefaults(defineProps<{
  path: string
  saving?: boolean
  saveLabel?: string
  placeholder?: string
}>(), {
  saving: false,
  saveLabel: t('确认保存'),
  placeholder: t('选择或输入目录路径'),
})

const emit = defineEmits<{ save: [path: string] }>()

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
  <div class="path-editor">
    <div class="muted label">{{ t('当前路径') }}</div>
    <div class="mono current">{{ path || t('未设置') }}</div>
    <n-input v-model:value="draft" clearable :placeholder="placeholder" />
    <div class="acts">
      <n-button secondary @click="browse"><FolderOpenOutline width="15" height="15" />{{ t('浏览…') }}</n-button>
      <n-button type="primary" :loading="saving" :disabled="!draft.trim() || draft.trim() === path" @click="save">
        <SaveOutline width="15" height="15" />{{ saveLabel }}
      </n-button>
    </div>
  </div>
</template>

<style scoped>
.label { margin-bottom: 6px; }
.current {
  margin-bottom: 12px;
  padding: 9px 12px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 8px;
  background: var(--oc2-sunken);
  color: var(--oc2-accent);
  overflow-wrap: anywhere;
}
.acts {
  display: flex;
  gap: 10px;
  margin-top: 12px;
  justify-content: flex-end;
}
</style>
