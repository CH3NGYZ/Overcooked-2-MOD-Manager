<script setup lang="ts">
import { computed, ref, shallowRef, watch } from 'vue'
import { NButton, NLog, NSelect, NTag, useDialog } from 'naive-ui'
import AppModal from './AppModal.vue'
import { t } from '../i18n'
import { backendMessageOf } from '../locale/backend'
import { api } from '../services/tauri'
import type { LogFileEntry, LogKind, RustLog } from '../types'
import { useNotify } from '../useNotify'

const kind = defineModel<LogKind | null>('kind', { required: true })
const titleKeys = { rust: 'tools.openRustLog', frontend: 'tools.openFrontendLog', crash: 'tools.openCrashLog' } as const
const title = computed(() => kind.value ? t(titleKeys[kind.value]) : '')
const dialog = useDialog()
const message = useNotify()
const files = shallowRef<LogFileEntry[]>([])
const options = computed(() => files.value.map(({ id, name }) => ({ label: name, value: id })))
const selectedId = ref<string | null>(null)
const log = shallowRef<RustLog | null>(null)
const listing = ref(false)
const reading = ref(false)
const working = ref<'copy' | 'clear' | null>(null)
let session = 0
let selection = 0

const levels = ['TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR'] as const
type LogLevel = 'all' | typeof levels[number]
const logLevel = ref<LogLevel>('INFO')
const logLevelOptions = computed(() => [
  { label: t('rename.allLevels'), value: 'all' },
  ...levels.map(value => ({ label: value, value })),
])
// Rust: [date][time][module][LEVEL]. Frontend: [date time][level].
// Only inspect the bracketed prefix so text in a message cannot become its level.
const levelPrefix = /^(?:\[[^\]]+\]){1,3}\[(TRACE|DEBUG|INFO|WARN|ERROR)\]\s/i
const filteredLog = computed(() => {
  const content = log.value?.content ?? ''
  const minimum = logLevel.value
  if (minimum === 'all') return content
  return content.split('\n').filter(line => {
    const level = levelPrefix.exec(line)?.[1].toUpperCase() as typeof levels[number] | undefined
    return level !== undefined && levels.indexOf(level) >= levels.indexOf(minimum)
  }).join('\n')
})
const filteredLineCount = computed(() => filteredLog.value.trimEnd() ? filteredLog.value.trimEnd().split('\n').length : 0)

function fail(reason: unknown) {
  message.error(backendMessageOf(reason))
}

watch(kind, async (current) => {
  const expected = ++session
  ++selection
  files.value = []
  selectedId.value = null
  log.value = null
  listing.value = false
  reading.value = false
  if (!current) return
  logLevel.value = current === 'crash' ? 'all' : 'INFO'
  listing.value = true
  try {
    const entries = await api.listLogFiles(current)
    if (expected !== session) return
    files.value = entries
    if (entries[0]) await selectLog(entries[0].id)
  } catch (reason) {
    if (expected === session) fail(reason)
  } finally {
    if (expected === session) listing.value = false
  }
}, { flush: 'sync' })

async function selectLog(id: string) {
  const current = kind.value
  if (!current || (id === selectedId.value && log.value)) return
  const expected = ++selection
  selectedId.value = id
  log.value = null
  reading.value = true
  try {
    const result = await api.readLog(current, id)
    if (expected === selection) log.value = result
  } catch (reason) {
    if (expected === selection) {
      selectedId.value = null
      fail(reason)
    }
  } finally {
    if (expected === selection) reading.value = false
  }
}

async function copyCurrentLog() {
  const current = kind.value
  const id = selectedId.value
  if (!current || !id || !log.value || working.value) return
  working.value = 'copy'
  try {
    await api.copyLogFile(current, id)
    message.success(t('tools.logFileCopied'))
  } catch (reason) { fail(reason) } finally { working.value = null }
}

function clearCurrentLog() {
  const current = kind.value
  const file = files.value.find(entry => entry.id === selectedId.value)
  if (!current || !file || !log.value || working.value) return
  const expected = selection
  dialog.warning({
    title: t('tools.clearLog'),
    content: t('tools.clearLogConfirm', { name: file.name }),
    positiveText: t('tools.clearLog'),
    negativeText: t('recycle.cancel'),
    onPositiveClick: async () => {
      if (working.value) return false
      working.value = 'clear'
      try {
        const result = await api.clearLog(current, file.id)
        if (selection === expected) log.value = result
        message.success(t('tools.logCleared'))
      } catch (reason) { fail(reason) } finally { working.value = null }
    },
  })
}
</script>

<template>
  <AppModal :show="kind !== null" card-class="log-dialog" fill max-width="min(880px, 90%)"
    @update:show="(visible: boolean) => { if (!visible) kind = null }">
    <template #header>
      <div class="log-heading">
        <span>{{ title }}</span>
        <n-select :value="selectedId" class="log-file-picker" size="small" :options="options"
          :placeholder="t('tools.logFile')" :aria-label="t('tools.logFile')" :loading="listing || reading"
          :disabled="!files.length || !!working" @update:value="selectLog" />
      </div>
    </template>
    <template #header-extra>
      <div class="log-head-extra">
        <n-select v-model:value="logLevel" size="small" class="log-level" :options="logLevelOptions" />
        <n-tag size="small" :bordered="false">{{ t('rename.lines', { count: filteredLineCount }) }}</n-tag>
      </div>
    </template>
    <div class="muted mono log-path">{{ log?.path }}</div>
    <div class="log-body">
      <n-log :log="filteredLog || (listing || reading ? '' : t('tools.noLogsAtThisLevel'))" :rows="60" :font-size="12" trim />
    </div>
    <template #footer>
      <n-button size="small" secondary :disabled="!log || !!working" :loading="working === 'copy'" @click="copyCurrentLog">
        {{ t('tools.copyLogFile') }}
      </n-button>
      <n-button size="small" type="error" secondary :disabled="!log || !!working" :loading="working === 'clear'" @click="clearCurrentLog">
        {{ t('tools.clearLog') }}
      </n-button>
    </template>
  </AppModal>
</template>

<style scoped>
.log-heading, .log-head-extra { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.log-file-picker { width: min(235px, 100%); }
.log-level { width: 110px; }
.log-path { margin: 0 0 10px; overflow-wrap: anywhere; }
.log-body { flex: 1 1 auto; min-height: 0; display: flex; flex-direction: column; overflow: hidden; }
.log-body :deep(.n-log) { flex: 1 1 auto; min-height: 0; }
</style>
