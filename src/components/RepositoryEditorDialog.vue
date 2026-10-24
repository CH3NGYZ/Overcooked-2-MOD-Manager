<script setup lang="ts">
import { Plus, Save, Trash2, X } from 'lucide-vue-next'
import { t } from '../i18n'
import type { RepositoryEditor } from '../composables/useRepositoryEditor'

defineProps<{ editor: RepositoryEditor; busy?: string }>()
</script>

<template>
  <div v-if="editor.showRepoEditor.value" class="modal-backdrop" @click.self="editor.showRepoEditor.value = false">
        <div class="modal repo-editor-modal">
          <div class="modal-head repo-editor-head">
            <div class="repo-editor-title">
              <h3>{{ t(editor.repoForm.id ? '编辑仓库' : '添加仓库') }}</h3>
              <label class="check-control mod-switch-control repo-editor-enabled">
                <input class="mod-switch-input" v-model="editor.repoForm.enabled" type="checkbox" />
                <span class="mod-switch" :class="{ 'mod-switch-on': editor.repoForm.enabled }" aria-hidden="true"><span class="mod-switch-thumb"></span></span>
                <span>{{ t('启用此仓库') }}</span>
              </label>
            </div>
            <button class="icon-button" @click="editor.showRepoEditor.value = false">
              <X :size="17" />
            </button>
          </div>
          <div class="repo-form-sections">
            <section class="repo-form-section">
              <div class="repo-form-section-title"><b>{{ t('仓库基本信息') }}</b><small>{{ t('配置仓库名称、来源平台、分组和地址') }}</small></div>
              <div class="form-grid">
                <label><span>{{ t('仓库名称') }}</span><input v-model="editor.repoForm.name" :placeholder="t('例如：我的 MOD 正式版')" @input="editor.updateRepositoryTranslation('name', $event)" /></label>
                <label><span>{{ t('来源平台') }}</span><select v-model="editor.repoForm.provider" :aria-label="t('来源平台')" @change="editor.providerChanged()">
                  <option value="gitee">Gitee</option>
                  <option value="github">GitHub</option>
                </select></label>
                <label><span>{{ t('仓库分组') }}</span><input v-model="editor.repoForm.group" :placeholder="t('例如：前置必装')" @input="editor.updateRepositoryTranslation('group', $event)" /></label>
                <label><span>{{ t('适用地区') }}</span><select v-model="editor.repoForm.region" :aria-label="t('适用地区')">
                  <option value="chinese_mainland">{{ t('中国大陆地区') }}</option>
                  <option value="international">{{ t('国际地区') }}</option>
                  <option value="">{{ t('全部地区') }}</option>
                </select></label>
                <div class="wide repository-homepage-field">
                  <div class="repository-homepage-fields">
                    <label class="repository-owner-field">
                      <span class="repository-field-label">{{ t('用户名') }}</span>
                      <input v-model="editor.repoForm.username" :placeholder="t('用户名')" :aria-label="t('用户名')" autocomplete="off" />
                    </label>
                    <span aria-hidden="true">/</span>
                    <label class="repository-name-field">
                      <span class="repository-field-label">{{ t('仓库名') }}</span>
                      <input v-model="editor.repoForm.repository" :placeholder="t('仓库名')" :aria-label="t('仓库名')" autocomplete="off" />
                    </label>
                  </div>
                </div>
              </div>
            </section>

            <section class="repo-form-section">
              <div class="repo-form-section-title"><b>{{ t('下载方式') }}</b><small>{{ t('选择通过 Release 或仓库文件下载') }}</small></div>
              <div class="form-grid">
                <label class="wide"><span>{{ t('下载方式') }}</span><select v-model="editor.repoForm.download_mode" :aria-label="t('下载方式')">
                  <option value="release">{{ t('Release 文件') }}</option>
                  <option value="repository_file">{{ t('仓库文件') }}</option>
                </select></label>
              </div>
            </section>

            <section v-if="editor.repoForm.download_mode === 'release'" class="repo-form-section">
              <div class="repo-form-section-title"><b>{{ t('Release 参数') }}</b><small>{{ t('配置版本筛选、请求和资产获取方式') }}</small></div>
              <div class="form-grid">
                <label><span>{{ t('版本频道') }}</span><select v-model="editor.repoForm.channel" :aria-label="t('版本频道')">
                  <option value="all">{{ t('全部版本') }}</option>
                  <option value="stable">{{ t('稳定版') }}</option>
                  <option value="beta">{{ t('测试版') }}</option>
                </select></label>
                <label><span>{{ t('每个仓库最多获取 Release 数量') }}</span><input v-model.number="editor.repoForm.max_release_count" type="number" min="1" max="100" /></label>
                <label><span>{{ t('下载线程数') }}</span><input v-model.number="editor.repoForm.download_threads" type="number" min="1" max="32" :disabled="editor.repoForm.provider === 'gitee'" /></label>
                <label class="wide"><span>{{ t('API 直连 Token') }} ({{ editor.repoForm.provider === 'gitee' ? 'Gitee' : 'GitHub' }})</span>
                  <input v-if="editor.repoForm.provider === 'gitee'" v-model="editor.repoForm.gitee_token" type="password" :placeholder="t('留空保持原设置')" />
                  <input v-else v-model="editor.repoForm.github_token" type="password" :placeholder="t('留空保持原设置')" />
                </label>
              </div>
            </section>

            <section v-else class="repo-form-section">
              <div class="repo-form-section-title"><b>{{ t('仓库文件参数') }}</b><small>{{ t('从指定分支下载单个文件，不读取 Release 版本') }}</small></div>
              <div class="form-grid">
                <label><span>{{ t('分支') }}</span><input v-model="editor.repoForm.branch" placeholder="main" /></label>
                <label><span>{{ t('下载线程数') }}</span><input v-model.number="editor.repoForm.download_threads" type="number" min="1" max="32" :disabled="editor.repoForm.provider === 'gitee'" /></label>
              </div>
            </section>

            <section v-if="editor.repoForm.download_mode === 'release'" class="repo-form-section">
              <div class="repo-asset-rules">
                <div class="repo-form-section-title repo-rule-title">
                  <div><b>{{ t('版本资产规则') }}</b><small>{{ t('支持精确文件名、通配符和正则表达式') }}</small></div>
                  <button class="ghost-button" @click="editor.addRule()"><Plus :size="14" />{{ t('添加规则') }}</button>
                </div>
                <div v-for="(rule, index) in editor.repoForm.asset_rules" :key="index" class="rule-row">
                  <select v-model="rule.type" :aria-label="t('规则类型')"><option value="exact">{{ t('精确') }}</option><option value="glob">{{ t('通配符') }}</option><option value="regex">{{ t('正则') }}</option></select>
                  <input v-model="rule.value" :aria-label="t('资产规则')" />
                  <button class="icon-button small danger-icon" :title="t('删除规则')" @click="editor.removeRule(index)"><Trash2 :size="14" /></button>
                </div>
              </div>
            </section>

            <section v-else class="repo-form-section">
              <div class="repo-form-section-title"><b>{{ t('仓库资产规则') }}</b><small>{{ t('仅支持具体文件，不支持通配和正则') }}</small></div>
              <div class="rule-row repository-file-rule-row">
                <span class="repository-file-rule-type">{{ t('精确') }}</span>
                <input v-model="editor.repoForm.file_path" :aria-label="t('文件相对路径')" :placeholder="t('例如：bin/Release/Example.dll')" />
              </div>
            </section>

            <section class="repo-form-section repo-binding-section">
              <div class="repo-form-section-title">
                <b>{{ t('本地 MOD 识别绑定') }}</b><small>{{ t('GUID、产品名称或原始文件名任一匹配即可绑定') }}</small><small>{{ t('填写版本号正则后，版本也必须匹配；留空则忽略版本条件') }}</small>
              </div>
              <div class="form-grid identity-grid">
                <label><span>{{ t('BepInEx.GUID') }}</span><input v-model="editor.repoForm.plugin_guid" :placeholder="t('可选')" /></label>
                <label><span>{{ t('DLL.产品名称') }}</span><input v-model="editor.repoForm.local_product_name" :placeholder="t('可选')" /></label>
                <label><span>{{ t('DLL.原始文件名') }}</span><input v-model="editor.repoForm.local_original_filename" :placeholder="t('可选')" /></label>
                <label><span>{{ t('版本号正则') }}</span><input v-model="editor.repoForm.version_regex" :placeholder="t('例如：^1\\.2\\.\\d+$')" /></label>
              </div>
            </section>

            <section class="repo-form-section repo-install-section">
              <div class="repo-form-section-title"><b>{{ t('安装目录') }}</b></div>
              <div class="form-grid">
                <label class="wide"><input v-model="editor.repoForm.install_target" :aria-label="t('安装目录')" :placeholder="t('留空按 ZIP 结构安装')" /></label>
              </div>
            </section>

            <section class="repo-form-section repo-note-section">
              <div class="repo-form-section-title">
                <b>{{ t('仓库备注') }}</b><small>{{ t('可选，多行备注') }}</small>
              </div>
              <textarea v-model="editor.repoForm.note" class="repository-note-input" rows="3" :placeholder="t('可选，多行备注')" @input="editor.updateRepositoryTranslation('note', $event)"></textarea>
            </section>
          </div>
          <div class="modal-actions"><button class="secondary-button" @click="editor.showRepoEditor.value = false">{{ t('取消')
              }}</button><button class="primary-button"
              :disabled="!editor.repoForm.name || !editor.repoForm.username.trim() || !editor.repoForm.repository.trim() || (editor.repoForm.download_mode === 'release' && (!editor.repoForm.asset_rules.length || editor.repoForm.asset_rules.some((rule) => !rule.value.trim()))) || (editor.repoForm.download_mode === 'repository_file' && (!editor.repoForm.branch.trim() || !editor.repoForm.file_path.trim())) || !!busy"
              @click="editor.saveRepo()">
              <Save :size="16" />{{ t('保存仓库') }}
            </button></div>
        </div>
      </div>
    
</template>
