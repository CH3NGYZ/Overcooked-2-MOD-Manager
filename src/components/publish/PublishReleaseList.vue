<script setup lang="ts">
import { computed } from 'vue'
import { NButton, NSpin, NTable, NTag } from 'naive-ui'
import { t } from '../../i18n'
import { latestVersionOfRelease, unregisteredReleaseVersions, versionRowsOf } from '../../mapPublishRelease'
import type { PublishRelease } from '../../types'
const props = defineProps<{ provider: 'github' | 'gitcode'; repositoryName: string; releases: PublishRelease[]; releasesLoading: boolean; deletingRelease: boolean }>()
const emit = defineEmits<{ create: []; open: [release: PublishRelease]; delete: [release: PublishRelease] }>()
const rows = computed(() => props.releases.map(release => ({
  release, latest: latestVersionOfRelease(release),
  versions: versionRowsOf(release).length, unregistered: unregisteredReleaseVersions(release).length,
})))
</script>
<template>
  <div class="panel">
    <div class="panel-head">
      <span class="muted mono">{{ repositoryName }}</span>
      <n-button size="small" type="primary" :disabled="releasesLoading || deletingRelease" @click="emit('create')">
        {{ t('mapPublish.newMapSet') }}
      </n-button>
    </div>
    <p v-if="provider === 'gitcode'" class="hint no-top">{{ t('mapPublish.releaseDeleteUnavailable') }}</p>
    <div v-if="releasesLoading" class="state-line"><n-spin size="small" /><span>{{ t('common.loading') }}</span></div>
    <p v-else-if="!rows.length" class="hint no-top">{{ t('mapPublish.mapSetEmpty') }}</p>
    <div v-else class="release-table-scroll">
      <n-table class="release-table" size="small" :single-line="false" :aria-label="t('mapPublish.mapSets')">
        <thead><tr>
          <th scope="col">{{ t('mapPublish.mapSetColumn') }}</th>
          <th scope="col">{{ t('mapPublish.levelSetNameZh') }}</th>
          <th scope="col">{{ t('mapPublish.levelSetName') }}</th>
          <th scope="col">{{ t('mapPublish.author') }}</th>
          <th scope="col">{{ t('mapPublish.latestVersion') }}</th>
          <th scope="col">{{ t('mapPublish.versionCount') }}</th>
          <th scope="col">{{ t('mapPublish.statusColumn') }}</th>
          <th scope="col">{{ t('mapPublish.actionsColumn') }}</th>
        </tr></thead>
        <tbody>
          <tr v-for="row in rows" :key="row.release.id" class="release-row">
            <th scope="row" class="release-name"><code class="release-tag mono">{{ row.release.tag }}</code></th>
            <td>{{ row.release.metadata?.level_set_name_zh || '—' }}</td>
            <td>{{ row.release.metadata?.level_set_name || '—' }}</td>
            <td>{{ row.release.metadata?.author || '—' }}</td>
            <td class="mono">{{ row.latest || '—' }}</td>
            <td class="mono">{{ row.versions }}</td>
            <td>
              <n-tag v-if="row.unregistered" size="small" type="warning" :bordered="false">
                {{ t('mapPublish.unregisteredVersions', { count: row.unregistered }) }}
              </n-tag>
              <span v-else-if="!row.release.metadata" class="muted">{{ t('mapPublish.noMetadata') }}</span>
              <span v-else class="muted">{{ t('mapPublish.stepDone') }}</span>
            </td>
            <td><div class="release-actions">
              <n-button size="small" :disabled="deletingRelease" @click="emit('open', row.release)">{{ t('mapPublish.mapManagement') }}</n-button>
              <n-button v-if="provider === 'github'" size="small" type="error" secondary :disabled="deletingRelease" @click="emit('delete', row.release)">{{ t('common.action.delete') }}</n-button>
            </div></td>
          </tr>
        </tbody>
      </n-table>
    </div>
  </div>
</template>
