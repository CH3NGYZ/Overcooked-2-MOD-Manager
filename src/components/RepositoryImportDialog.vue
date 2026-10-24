<script setup lang="ts">
import { Check, CheckCircle2, Copy, Download, Search, X } from 'lucide-vue-next'
import { t } from '../i18n'
import type { ImportExport } from '../composables/useImportExport'

defineProps<{ io: ImportExport }>()
</script>

<template>
  <div v-if="io.showImport.value" class="modal-backdrop" @click.self="io.showImport.value = false">
        <div class="modal import-modal">
          <div class="modal-head">
            <div>
              <h3>{{ t('导入仓库配置') }}</h3>
            </div><button class="icon-button" @click="io.showImport.value = false">
              <X :size="17" />
            </button>
          </div>
          <div class="import-actions"><button class="secondary-button" @click="io.importFromFile()">
              <Download :size="15" />{{ t('选择 .oc2repo.toml') }}
            </button><button class="secondary-button" @click="io.importFromClipboard()">
              <Copy :size="15" />{{ t('读取剪贴板') }}
            </button></div><textarea v-model="io.importText.value" :placeholder="t('粘贴仓库 TOML 内容')"></textarea><button
            class="ghost-button full-button" :disabled="!io.importText.value" @click="io.previewImport()">
            <Search :size="15" />{{ t('检查导入内容') }}
          </button>
          <div v-if="io.importPreview.value" class="import-preview">
            <div class="preview-state" :class="io.importPreview.value.valid ? 'success' : 'danger'">
              <CheckCircle2 :size="16" />{{ t(io.importPreview.value.valid ? '格式有效' : '格式无效') }}
            </div>
            <div class="preview-grid"><span>{{ t('新增') }} {{ io.importPreview.value.additions.length }}</span><span>{{ t('重复') }}
                {{
                  io.importPreview.value.duplicates.length }}</span><span>{{ t('冲突') }} {{ io.importPreview.value.conflicts.length
                }}</span></div><small v-if="io.importPreview.value.errors.length">{{ io.importPreview.value.errors.map(t).join('；')
                }}</small>
            <div v-if="io.importPreview.value.valid" class="modal-actions"><button class="secondary-button"
                @click="io.applyImport('keep_both')">{{ t('合并保存') }}</button><button class="primary-button"
                @click="io.applyImport('overwrite')">
                <Check :size="15" />{{ t('覆盖保存') }}
              </button></div>
          </div>
        </div>
      </div>
    
</template>
