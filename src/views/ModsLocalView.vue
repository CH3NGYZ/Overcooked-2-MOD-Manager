<script setup lang="ts">
import { t } from '../i18n'
import { computed, onMounted, ref } from 'vue'
import { useDialog, useMessage } from 'naive-ui'
import { NButton, NCollapse, NCollapseItem, NEmpty, NFloatButton, NFloatButtonGroup, NSpin, NSwitch, NTag } from 'naive-ui'
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

/**
 * 两级列表：MOD 包 → 子 MOD。
 * 有 package_id 的按包归组；未归属的 MOD 各自成为单成员「包」，保证一条都不丢。
 */
interface ModPackageNode {
  key: string
  name: string
  mods: InstalledMod[]
  count: number
  updateCount: number
  enabled: boolean
  partial: boolean
}

const packages = computed<ModPackageNode[]>(() => {
  const buckets = new Map<string, InstalledMod[]>()
  for (const mod of sortedMods.value) {
    const key = mod.package_id ? `pkg:${mod.package_id}` : `mod:${mod.dll_file}`
    const list = buckets.get(key)
    if (list) list.push(mod)
    else buckets.set(key, [mod])
  }
  return [...buckets.entries()].map(([key, list]) => {
    const enabled = list.filter((mod) => mod.enabled).length
    return {
      key,
      name: list[0]?.package_name || modTitle(list[0]),
      mods: list,
      count: list.length,
      updateCount: list.filter((mod) => mod.update_available).length,
      enabled: enabled === list.length,
      partial: enabled > 0 && enabled < list.length,
    }
  })
})

const openPackages = ref<string[]>([])
const allPackageKeys = computed(() => packages.value.map((node) => node.key))
const packagesExpanded = computed(() =>
  openPackages.value.length ? openPackages.value : allPackageKeys.value,
)

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
      <span class="muted">
        {{ t('共 {count} 个 · 启用 {enabled} · 可更新 {updates} · 排序：{sort}',
          { count: mods.length, enabled: enabledCount, updates: updateCount, sort: sortLabel }) }}
      </span>
    </div>

    <n-empty v-if="!loading && !sortedMods.length" :description="t('尚未安装任何 MOD')" class="empty" />

    <!-- 两级折叠：MOD 包 → 子 MOD -->
    <n-collapse v-else :expanded-names="packagesExpanded" class="package-collapse"
      @update:expanded-names="(names: string[]) => (openPackages = names)">
      <n-collapse-item v-for="node in packages" :key="node.key" :name="node.key">
        <template #header>
          <div class="package-head">
            <b>{{ node.name }}</b>
            <n-tag size="tiny" :bordered="false">{{ node.count }}</n-tag>
            <n-tag v-if="node.updateCount" size="tiny" type="warning" :bordered="false">
              {{ t('{count} 个可更新', { count: node.updateCount }) }}
            </n-tag>
            <n-tag v-if="!node.enabled && !node.partial" size="tiny" :bordered="false">{{ t('已禁用') }}</n-tag>
            <n-tag v-else-if="node.partial" size="tiny" type="info" :bordered="false">{{ t('部分启用') }}</n-tag>
          </div>
        </template>

        <div :class="layout === 'cards' ? 'mod-grid' : 'mod-rows'">
          <article v-for="mod in node.mods" :key="mod.dll_file" class="mod-card" :class="{ disabled: !mod.enabled }">
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
              <span class="muted ellipsis">{{ mod.name }}</span>
              <span class="acts">
                <n-button size="tiny" quaternary @click="toggleExpand(mod.dll_file)">
                  <ChevronUpOutline v-if="expanded[mod.dll_file]" width="13" height="13" />
                  <ChevronDownOutline v-else width="13" height="13" />{{ t('详情') }}
                </n-button>
                <n-button size="tiny" quaternary type="error" :disabled="game?.kitchen_running" @click="remove(mod)">
                  <TrashOutline width="13" height="13" />
                </n-button>
              </span>
            </div>
            <div v-if="expanded[mod.dll_file]" class="detail mono">
              <div>{{ t('GUID：{guid}', { guid: mod.guid || '—' }) }}</div>
              <div>{{ t('文件：{file}', { file: mod.dll_file }) }}</div>
              <div v-if="mod.load_warning" class="warn">{{ t('警告：{warning}', { warning: mod.load_warning }) }}</div>
            </div>
          </article>
        </div>
      </n-collapse-item>
    </n-collapse>
  </n-spin>

  <n-float-button-group position="fixed" bottom="28" right="32" gap="10">
    <n-float-button :tooltip="t('切换布局（卡片 / 横排）')" @click="setLayout(layout === 'cards' ? 'rows' : 'cards')">
      <ReorderFourOutline v-if="layout === 'cards'" width="17" height="17" />
      <GridOutline v-else width="17" height="17" />
    </n-float-button>
    <n-float-button :tooltip="`${t('排序')}：${sortLabel}`" @click="cycleSort">
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
/* 两级折叠：包标题行 + 内部卡片网格 / 横排列表 */
.package-collapse { width: 100%; }
.package-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
.package-head b {
  font-size: 14px;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mod-grid {
  display: grid; grid-template-columns: repeat(auto-fill, minmax(300px, 1fr)); gap: 12px;
  margin: 2px 0 6px 14px;
}
.mod-rows { display: flex; flex-direction: column; gap: 10px; margin: 2px 0 6px 14px; }
.mod-rows .mod-card { width: 100%; }
.mod-card {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: 11px 13px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
  transition: border-color .16s ease, background-color .16s ease;
}
.mod-card:hover { border-color: var(--oc2-accent); background: var(--oc2-row-hover); }
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
</style>
