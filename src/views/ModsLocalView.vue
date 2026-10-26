<script setup lang="ts">
import { t } from '../i18n'
import { computed, onMounted, ref } from 'vue'
import { useDialog, useMessage } from 'naive-ui'
import { NButton, NCard, NEmpty, NFloatButton, NFloatButtonGroup, NSpin, NSwitch, NTag } from 'naive-ui'
import { ChevronDownOutline, ChevronUpOutline, GridOutline, ReorderFourOutline, SwapVerticalOutline, TrashOutline } from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { useDashboard } from '../composables/useDashboardStore'
import type { InstalledMod } from '../types'

const message = useMessage()
const dialog = useDialog()
const { load, loading, mods, game } = useDashboard()

const layout = ref<'cards' | 'rows'>((localStorage.getItem('v2-mods-layout') as 'cards' | 'rows') || 'cards')
const sortBy = ref<'name' | 'version' | 'enabled'>('name')
const sortLabels = { name: t('名称'), version: t('版本'), enabled: '启用' } as const
function cycleSort() {
  const order = ['name', 'version', 'enabled'] as const
  sortBy.value = order[(order.indexOf(sortBy.value) + 1) % order.length]
}
const sortLabel = computed(() => sortLabels[sortBy.value])
const busy = ref<Record<string, boolean>>({})
const expanded = ref<Record<string, boolean>>({})

onMounted(() => { void load() })

function setLayout(value: 'cards' | 'rows') {
  layout.value = value
  localStorage.setItem('v2-mods-layout', value)
}

const sortedMods = computed(() => {
  const list = [...mods.value]
  if (sortBy.value === 'name') list.sort((a, b) => a.name.localeCompare(b.name))
  if (sortBy.value === 'version') list.sort((a, b) => (b.version ?? '').localeCompare(a.version ?? ''))
  if (sortBy.value === 'enabled') list.sort((a, b) => Number(b.enabled) - Number(a.enabled))
  return list
})

const enabledCount = computed(() => mods.value.filter((mod) => mod.enabled).length)
const updateCount = computed(() => mods.value.filter((mod) => mod.update_available).length)

function modTitle(mod: InstalledMod) {
  return mod.title || mod.product_name || mod.name
}

function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
}

async function toggle(mod: InstalledMod, enabled: boolean) {
  busy.value[mod.dll_file] = true
  try {
    const result = await api.toggleLocalMod(mod.dll_file, enabled)
    result.ok ? message.success(result.message || (enabled ? '已启用' : t('已禁用'))) : message.error(result.message)
    await load()
  } catch (reason) { fail(reason) } finally { busy.value[mod.dll_file] = false }
}

function remove(mod: InstalledMod) {
  dialog.warning({
    title: t('删除 MOD'),
    content: t('「{name}」将被移入回收站，可在回收站中恢复。继续？', { name: modTitle(mod) }),
    positiveText: t('移入回收站'),
    negativeText: t('取消'),
    async onPositiveClick() {
      busy.value[mod.dll_file] = true
      try {
        const result = await api.uninstallLocalMod(mod.dll_file)
        if (result.requires_confirmation) {
          dialog.info({
            title: t('该 MOD 属于 MOD 包'),
            content: t('它与 {count} 个子 MOD 同属一个包，一并删除？', { count: result.package_mod_names?.length ?? 0 }),
            positiveText: t('整包删除'),
            negativeText: t('取消'),
            async onPositiveClick() {
              const packageResult = await api.uninstallLocalMod(mod.dll_file, true)
              packageResult.ok ? message.success(packageResult.message || '已移入回收站') : message.error(packageResult.message)
              await load()
            },
          })
        } else {
          result.ok ? message.success(result.message || '已移入回收站') : message.error(result.message)
        }
        await load()
      } catch (reason) { fail(reason) } finally { busy.value[mod.dll_file] = false }
    },
  })
}

function toggleExpand(dll: string) {
  expanded.value[dll] = !expanded.value[dll]
}
</script>

<template>
  <n-spin :show="loading">
    <div class="toolbar">
      <span class="muted">共 {{ mods.length }} 个 · 启用 {{ enabledCount }} · 可更新 {{ updateCount }} · 排序：{{ sortLabel }}</span>


    </div>

    <n-empty v-if="!loading && !sortedMods.length" :description="t('尚未安装任何 MOD')" class="empty" />

    <!-- 卡片布局 -->
    <div v-if="layout === 'cards'" class="mod-grid">
      <n-card v-for="mod in sortedMods" :key="mod.dll_file" class="mod-card" :class="{ disabled: !mod.enabled }">
        <div class="card-head">
          <b class="title" :title="modTitle(mod)">{{ modTitle(mod) }}</b>
          <n-tag v-if="mod.update_available" size="tiny" type="warning" :bordered="false">{{ t('有更新') }}</n-tag>
          <n-tag v-else-if="mod.labels?.includes('latest')" size="tiny" type="success" :bordered="false">{{ t('最新版') }}</n-tag>
        </div>
        <div class="card-meta">
          <span class="mono">v{{ mod.version || '—' }}</span>
          <n-switch size="small" :value="mod.enabled" :loading="!!busy[mod.dll_file]"
            @update:value="(value: boolean) => toggle(mod, value)" />
        </div>
        <div class="card-foot">
          <span class="muted ellipsis">{{ modTitle(mod) === mod.name ? mod.product_name || mod.name : mod.name }}</span>
          <span class="acts">
            <n-button size="tiny" quaternary @click="toggleExpand(mod.dll_file)">
              <ChevronUpOutline v-if="expanded[mod.dll_file]" width="13" height="13" />
              <ChevronDownOutline v-else width="13" height="13" />{{ t('详情') }}</n-button>
            <n-button size="tiny" quaternary type="error" :disabled="game?.kitchen_running" @click="remove(mod)">
              <TrashOutline width="13" height="13" />
            </n-button>
          </span>
        </div>
        <div v-if="expanded[mod.dll_file]" class="detail mono">
          <div>GUID：{{ mod.guid || '—' }}</div>
          <div>文件：{{ mod.dll_file }}</div>
          <div v-if="mod.package_name">MOD 包：{{ mod.package_name }}（{{ mod.package_mod_count }} 个）</div>
          <div v-if="mod.load_warning" class="warn">警告：{{ mod.load_warning }}</div>
        </div>
      </n-card>
    </div>

    <!-- 横排布局 -->
    <n-card v-else class="rows-card">
      <div v-for="mod in sortedMods" :key="mod.dll_file" class="row" :class="{ disabled: !mod.enabled }">
        <n-switch size="small" :value="mod.enabled" :loading="!!busy[mod.dll_file]"
          @update:value="(value: boolean) => toggle(mod, value)" />
        <b class="row-title" :title="modTitle(mod)">{{ modTitle(mod) }}</b>
        <span class="mono row-version">v{{ mod.version || '—' }}</span>
        <n-tag v-if="mod.update_available" size="tiny" type="warning" :bordered="false">{{ t('有更新') }}</n-tag>
        <n-tag v-else-if="mod.labels?.includes('latest')" size="tiny" type="success" :bordered="false">{{ t('最新版') }}</n-tag>
        <span v-if="mod.load_warning" class="muted">{{ mod.load_warning }}</span>
        <span class="row-acts">
          <n-button size="tiny" quaternary @click="toggleExpand(mod.dll_file)">
            <ChevronUpOutline v-if="expanded[mod.dll_file]" width="13" height="13" />
            <ChevronDownOutline v-else width="13" height="13" />{{ t('详情') }}</n-button>
          <n-button size="tiny" quaternary type="error" :disabled="game?.kitchen_running" @click="remove(mod)">
            <TrashOutline width="13" height="13" />
          </n-button>
        </span>
      </div>
      <template v-for="mod in sortedMods" :key="'d-' + mod.dll_file">
        <div v-if="expanded[mod.dll_file]" class="detail mono inset">
          <div>GUID：{{ mod.guid || '—' }}</div>
          <div>文件：{{ mod.dll_file }}</div>
          <div v-if="mod.package_name">MOD 包：{{ mod.package_name }}（{{ mod.package_mod_count }} 个）</div>
          <div v-if="mod.load_warning" class="warn">警告：{{ mod.load_warning }}</div>
        </div>
      </template>
    </n-card>
  </n-spin>
    <n-float-button-group position="fixed" bottom="28" right="32" gap="10">
      <n-float-button :tooltip="t('切换布局（卡片 / 横排）')" @click="setLayout(layout === 'cards' ? 'rows' : 'cards')">
        <ReorderFourOutline v-if="layout === 'cards'" width="17" height="17" />
        <GridOutline v-else width="17" height="17" />
      </n-float-button>
      <n-float-button :tooltip="`排序：${sortLabel}`" @click="cycleSort">
        <SwapVerticalOutline width="17" height="17" />
      </n-float-button>
    </n-float-button-group>
  </template>

<style scoped>
.toolbar {
  display: flex; align-items: center; gap: 12px; margin-bottom: 14px;
}
.toolbar > .muted { margin-right: auto; }
.seg { display: flex; gap: 6px; }
.empty { margin: 60px 0; }
.mod-grid {
  display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px;
  max-width: 1180px;
}
.mod-card.disabled { opacity: .6; }
.card-head { display: flex; align-items: center; gap: 8px; }
.title { font-size: 13.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.card-meta {
  display: flex; align-items: center; justify-content: space-between; margin: 8px 0;
}
.card-meta .mono { color: var(--oc2-accent); }
.card-foot { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.card-foot .ellipsis { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.acts { display: flex; gap: 4px; flex: 0 0 auto; }
.detail {
  margin-top: 10px; padding-top: 10px; border-top: 1px dashed var(--oc2-row-line);
  color: var(--oc2-text-3); display: grid; gap: 3px;
}
.detail .warn { color: var(--oc2-accent); }
.rows-card { max-width: 1180px; }
.row {
  display: flex; align-items: center; gap: 14px; padding: 9px 2px;
  border-bottom: 1px dashed var(--oc2-rail-line);
}
.row:last-child { border-bottom: 0; }
.row.disabled { opacity: .6; }
.row-title {
  flex: 1; min-width: 0; font-size: 13.5px;
  overflow: hidden; text-overflow: ellipsis; white-space: nowrap;
}
.row-version { color: var(--oc2-accent); flex: 0 0 auto; }
.row-acts { display: flex; gap: 4px; flex: 0 0 auto; }
.detail.inset { margin: 0 0 10px 34px; }
</style>
