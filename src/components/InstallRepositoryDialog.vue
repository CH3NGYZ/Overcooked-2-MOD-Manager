<script setup lang="ts">
  import { Download, X } from 'lucide-vue-next'
  import { t } from '../i18n'
  import { repositoryMarkdownBaseUrl } from '../markdown'
    import { installReleases } from '../repositoryAssets'
  import { formatAssetMetadata } from '../formatting'
  import MarkdownContent from './MarkdownContent.vue'
  import type { ModOperations } from '../composables/useModOperations'

  defineProps<{ ops: ModOperations; busy?: string }>()
  </script>

  <template>
    <div v-if="ops.showInstall.value" class="modal-backdrop" @click.self="ops.closeInstallDialog()">
      <div class="modal install-modal">
        <div class="modal-head">
          <div>
            <h3>{{ ops.showInstall.value.download_mode === 'release' ? t('选择版本和文件') : t('选择下载方式') }}</h3>
          </div><button class="icon-button" @click="ops.closeInstallDialog()">
            <X :size="17" />
          </button>
        </div><template v-if="ops.showInstall.value.download_mode === 'release'"><label class="install-field"><span>{{ t('Release 版本') }}</span><select v-model="ops.selectedInstallTag.value" :aria-label="t('Release 版本')" @change="ops.selectInstallRelease()">
            <option v-for="release in installReleases(ops.showInstall.value)" :key="release.tag_name" :value="release.tag_name">{{
              release.version || release.tag_name }} · {{ release.name }}
            </option>
          </select></label>
        <div v-if="ops.selectedInstallRelease(ops.showInstall.value)" class="install-release-log">
          <div class="install-release-log-head"><strong>{{ ops.selectedInstallRelease(ops.showInstall.value)?.name || ops.selectedInstallRelease(ops.showInstall.value)?.tag_name }}</strong><span>{{ ops.selectedInstallRelease(ops.showInstall.value)?.version || ops.selectedInstallRelease(ops.showInstall.value)?.tag_name }}</span></div>
          <MarkdownContent class="install-release-log-body" breaks
            :source="ops.selectedInstallRelease(ops.showInstall.value)?.body || t('Release 没有更新日志。')" :base-url="repositoryMarkdownBaseUrl(ops.showInstall.value)" />
        </div>
        <label class="install-field"><span>{{ t('下载文件') }}</span><small class="install-field-hint">{{ t('仅显示符合当前仓库资产规则的文件。') }}</small><select
            v-model="ops.selectedInstallAsset.value" :aria-label="t('下载文件')">
            <option v-for="asset in ops.selectedInstallAssets(ops.showInstall.value)" :key="asset.name" :value="asset.name">{{
              asset.name
              }} · {{ formatAssetMetadata(asset) }}</option>
          </select></label></template><div v-else class="install-source-file"><span>{{ t('仓库文件') }}</span><b>{{ ops.showInstall.value.file_path }}</b></div>
        <section v-if="ops.assetProxySelectionVisible(ops.showInstall.value)" class="install-proxy-choice">
          <div class="install-field-label">{{ t('GitHub 资产下载方式') }}</div>
          <div class="install-proxy-modes">
            <button type="button" :class="['proxy-mode-button', { active: ops.selectedAssetProxyMode.value === 'auto_proxy' }]" :aria-pressed="ops.selectedAssetProxyMode.value === 'auto_proxy'" @click="ops.selectedAssetProxyMode.value = 'auto_proxy'">{{ t('自动代理') }}</button>
            <button type="button" :class="['proxy-mode-button', { active: ops.selectedAssetProxyMode.value === 'auto_direct' }]" :aria-pressed="ops.selectedAssetProxyMode.value === 'auto_direct'" @click="ops.selectedAssetProxyMode.value = 'auto_direct'">{{ t('自动直连') }}</button>
            <button type="button" :class="['proxy-mode-button', { active: ops.selectedAssetProxyMode.value === 'direct' }]" @click="ops.selectedAssetProxyMode.value = 'direct'">{{ t('直链') }}</button>
            <button type="button" :class="['proxy-mode-button', { active: ops.selectedAssetProxyMode.value === 'fixed' }]" @click="ops.selectedAssetProxyMode.value = 'fixed'">{{ t('手动选择') }}</button>
          </div>
          <select v-if="ops.selectedAssetProxyMode.value === 'fixed'" v-model="ops.selectedAssetProxyId.value" class="install-proxy-select" :aria-label="t('手动选择')">
            <option value="">{{ t('请选择代理') }}</option>
            <option v-for="proxy in ops.assetProxyEntries(ops.showInstall.value)" :key="proxy.id" :value="proxy.id">{{ proxy.name }}<template v-if="proxy.regions.length"> · {{ proxy.regions.join('、') }}</template></option>
          </select>
        </section>
        <div class="modal-actions"><button class="secondary-button" @click="ops.closeInstallDialog()">{{ t('取消')
            }}</button><button class="primary-button" :disabled="(ops.showInstall.value.download_mode === 'release' && !ops.selectedInstallAsset.value) || (ops.assetProxySelectionVisible(ops.showInstall.value) && ops.selectedAssetProxyMode.value === 'fixed' && !ops.selectedAssetProxyId.value) || !!busy" @click="ops.confirmInstall()">
            <Download :size="15" />{{ t('下载并安装') }}
          </button></div>
      </div>
    </div>
  </template>
