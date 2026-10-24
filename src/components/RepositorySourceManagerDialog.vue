<script setup lang="ts">
import { ChevronRight, GripVertical, Pencil, Plus, Save, Trash2, X } from 'lucide-vue-next'
import { t, localizedRepositorySourceName } from '../i18n'
import { repositorySourceRegionLabel } from '../repositorySources'
import type { RepositorySourceManager } from '../composables/useRepositorySourceManager'

defineProps<{ manager: RepositorySourceManager; busy?: string }>()
</script>

<template>
    <div v-if="manager.showRepositorySource.value" class="modal-backdrop" @click.self="manager.closeRepositorySourceManager()">
      <div class="modal source-manager-modal">
        <div class="modal-head">
          <div>
            <h3>{{ t('管理仓库源') }}</h3>
            <p class="muted-text">{{ t('先编辑仓库源，点击保存后统一请求数据') }}</p>
          </div><button class="icon-button" :disabled="!!busy" @click="manager.closeRepositorySourceManager()">
            <X :size="17" />
          </button>
        </div>
        <div class="source-manager-list">
          <div v-for="entry in manager.repositorySourceManagerEntries.value" :key="entry.source.id"
            :class="['source-manager-row', { 'source-manager-child-row': entry.depth > 0 }]"
            :draggable="entry.depth === 0 && !entry.source.parent_id && !entry.source.builtin"
            @dragstart="manager.startSourceDrag(entry.source.id)" @dragover.prevent
            @drop="manager.dropSource(entry.source.id)">
            <GripVertical v-if="entry.depth === 0" :size="17" class="source-drag-handle" />
            <ChevronRight v-else :size="15" class="source-child-marker" />
            <div><b>{{ localizedRepositorySourceName(entry.source) }}</b><small>{{ t('仓库唯一ID') }}: {{ entry.source.id }}</small><small v-if="entry.depth === 0">{{ t('当前MOD下载渠道') }}: {{ repositorySourceRegionLabel(entry.source.region) }}</small><small>{{ entry.source.url }}</small></div>
            <span v-if="entry.source.builtin" class="badge muted">{{ t('内置') }}</span>
            <button v-if="entry.depth === 0 && !entry.source.builtin" class="icon-button small" :title="t('编辑仓库源')"
              @click="manager.editRepositorySource(entry.source)">
              <Pencil :size="14" />
            </button>
            <button v-if="entry.depth === 0 && !entry.source.builtin" class="icon-button small danger-icon" :title="t('删除仓库源')"
              @click="manager.removeRepositorySource(entry.source)">
              <Trash2 :size="14" />
            </button>
          </div>
        </div>
        <div class="source-manager-toolbar"><button class="secondary-button"
            :disabled="!!busy || manager.repositorySourceEditorOpen.value" @click="manager.startAddingRepositorySource()">
            <Plus :size="15" />{{ t('添加仓库源') }}
          </button></div>
        <div v-if="manager.repositorySourceEditorOpen.value" class="source-editor-fields"><label><span>{{ t('仓库唯一ID') }}</span><input
              v-model="manager.repositorySourceUniqueId.value" :disabled="!!manager.editingRepositorySourceId.value" :placeholder="t('例如：my-source-id')" /></label><label><span>{{ t('仓库源昵称') }}</span><input
              v-model="manager.repositorySourceNickname.value" :placeholder="t('例如：我的仓库源')" /></label><label><span>{{ t('仓库源地址')
              }}</span><input v-model="manager.repositorySourceUrl.value"
              placeholder="https://example.com/repositories.oc2repo.toml" /></label><label><span>{{ t('适用地区') }}</span><select v-model="manager.repositorySourceRegion.value"><option value="chinese_mainland">{{ t('中国大陆地区') }}</option><option value="international">{{ t('国际地区') }}</option><option value="">{{ t('全部地区') }}</option></select></label>
          <div class="source-editor-actions"><button class="ghost-button" @click="manager.resetRepositorySourceEditor()">{{
            t('取消')
              }}</button><button class="secondary-button"
              :disabled="!manager.repositorySourceUniqueId.value.trim() || !manager.repositorySourceNickname.value.trim() || !manager.repositorySourceUrl.value.trim()"
              @click="manager.applyRepositorySourceDraft()">
              <Save :size="15" />{{ t('应用修改') }}
            </button></div>
        </div>
        <div class="modal-actions"><button class="secondary-button" :disabled="!!busy"
            @click="manager.closeRepositorySourceManager()">{{ t('关闭') }}</button><button class="primary-button"
            :disabled="!!busy || !manager.repositorySourceDraft.value.length" @click="manager.saveRepositorySourceChanges()">
            <Save :size="16" />{{ t('保存仓库源') }}
          </button></div>
      </div>
    </div>
</template>
