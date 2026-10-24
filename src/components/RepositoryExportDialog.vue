<script setup lang="ts">
import { Copy, Upload, X } from 'lucide-vue-next'
import { t } from '../i18n'
import type { ImportExport } from '../composables/useImportExport'

defineProps<{ io: ImportExport }>()
</script>

<template>
  <div v-if="io.showExport.value" class="modal-backdrop" @click.self="io.showExport.value = false">
          <div class="modal export-modal">
            <div class="modal-head">
              <div>
                <h3>{{ t('导出仓库配置') }}</h3>
                <p class="muted-text">{{ t('选择导出方式') }}</p>
              </div><button class="icon-button" @click="io.showExport.value = false">
                <X :size="17" />
              </button>
            </div>
            <div class="modal-actions"><button class="secondary-button" @click="io.copyRepositories()">
                <Copy :size="15" />{{ t('复制文件内容') }}
              </button><button class="primary-button" @click="io.continueExportRepositories()">
                <Upload :size="15" />{{ t('继续导出') }}
              </button></div>
          </div>
        </div>
      
</template>
