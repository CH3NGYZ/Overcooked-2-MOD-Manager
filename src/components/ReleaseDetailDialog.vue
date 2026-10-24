<script setup lang="ts">
import { X } from 'lucide-vue-next'
import { t } from '../i18n'
import { formatDate as formatDateForLocale } from '../formatting'
import { formatAssetMetadata } from '../formatting'
import { locale } from '../i18n'
import type { Repository } from '../types'

const props = defineProps<{ repo: Repository | null }>()
const emit = defineEmits<{ close: [] }>()
function latest(repo: Repository | null) { return repo?.cached_releases?.[0] }
function formatDate(value?: string) { return formatDateForLocale(value, locale.value) }
void props
</script>

<template>
  <div v-if="repo" class="modal-backdrop" @click.self="emit('close')">
        <div class="modal release-modal">
          <div class="modal-head">
            <div>
              <h3>{{ repo.name }}</h3>
            </div><button class="icon-button" @click="emit('close')">
              <X :size="17" />
            </button>
          </div>
          <div v-if="latest(repo)" class="release-detail">
            <div class="release-detail-meta"><span class="release-version">{{ latest(repo)?.version ||
              latest(repo)?.tag_name }}</span><span>{{
                  formatDate(latest(repo)?.published_at) }}</span><span class="badge"
                :class="latest(repo)?.prerelease ? 'warning' : 'success'">{{
                  t(latest(repo)?.prerelease ? 'BETA' : 'STABLE') }}</span></div>
            <h4>{{ latest(repo)?.name || latest(repo)?.tag_name }}</h4>
            <p class="release-body">{{ latest(repo)?.body || t('Release 没有更新日志。') }}</p>
            <div class="asset-list">
              <div v-for="asset in latest(repo)?.assets" :key="asset.name"><span>{{ asset.name
                  }}</span><small>{{ formatAssetMetadata(asset) }}</small></div>
            </div>
          </div>
        </div>
      </div>
    
</template>
