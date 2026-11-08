<script setup lang="ts">
import { computed } from 'vue'
import { api } from '../services/tauri'
import { ownerHomepage, providerLabel, repositoryHomepage, sourceRepository, type RepositoryCoordinates } from '../providers'
import { backendMessageOf } from '../locale/backend'
import { useNotify } from '../useNotify'

const props = defineProps<{ label: string; items?: readonly RepositoryCoordinates[]; sourceUrl?: string }>()
const repository = computed(() => sourceRepository(props.items ?? [], props.sourceUrl))
const message = useNotify()
async function open(url: string) {
  try {
    const result = await api.openReleasePage(url)
    if (!result.ok) message.error(backendMessageOf(result.message))
  } catch (error) { message.error(backendMessageOf(error)) }
}
</script>

<template>
  <span class="source-title">
    <b>{{ label }}</b>
    <span v-if="repository" class="source-origin">
      <span> · {{ providerLabel(repository.provider) }} · </span>
      <a :href="ownerHomepage(repository.provider, repository.username)"
        @click.stop.prevent="open(ownerHomepage(repository.provider, repository.username))">{{ repository.username }}</a>
      <span> · </span>
      <a :href="repositoryHomepage(repository.provider, repository.username, repository.repository)"
        @click.stop.prevent="open(repositoryHomepage(repository.provider, repository.username, repository.repository))">{{ repository.repository }}</a>
    </span>
  </span>
</template>

<style scoped>
.source-title { min-width: 0; overflow-wrap: anywhere; }
.source-title b { font-size: 14px; color: var(--oc2-text-1); }
.source-origin { color: var(--oc2-text-3); font-size: 12px; font-weight: normal; }
.source-origin a { color: inherit; text-decoration: none; }
.source-origin a:hover, .source-origin a:focus-visible { text-decoration: underline; }
</style>
