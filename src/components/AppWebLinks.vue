<script setup lang="ts">
import { onBeforeUnmount, onMounted } from 'vue'
import { backendMessageOf } from '../locale/backend'
import { isTauri } from '../runtime'
import { api } from '../services/tauri'
import { useNotify } from '../useNotify'
import { listenForWebLinks } from '../webLinks'

const notify = useNotify()
let stop: (() => void) | undefined
onMounted(() => {
  if (!isTauri()) return
  stop = listenForWebLinks(document, (url) => {
    void api.openExternalPage(url).catch(reason => notify.error(backendMessageOf(reason)))
  }, () => notify.error('error.shell.webPageInvalid'))
})
onBeforeUnmount(() => stop?.())
</script>

<template></template>
