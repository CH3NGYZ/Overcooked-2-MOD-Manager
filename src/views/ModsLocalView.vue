<script setup lang="ts">
import { t, locale } from '../i18n'
import { computed, onMounted, ref } from 'vue'
import { useDialog, useMessage } from 'naive-ui'
import { NButton, NCollapse, NCollapseItem, NEmpty, NFloatButton, NSpin, NSwitch, NTag } from 'naive-ui'
import {
  FolderOpenOutline, SettingsOutline, SwapVerticalOutline, TrashOutline,
} from '@vicons/ionicons5'
import { api } from '../services/tauri'
import { compareVersions, repositoryForMod } from '../modStatus'
import { installedModTags } from '../statusTags'
import { useFocusHighlight, useRepositoryLink } from '../crossLink'
import { useDashboard } from '../composables/useDashboardStore'
import ModConfigEditor from '../components/ModConfigEditor.vue'
import type { InstalledMod, Repository } from '../types'

const message = useMessage()
const dialog = useDialog()
const { load, ensureLoaded, loading, mods, game, config } = useDashboard()

/** 状态标签跳转：点标签跳到 MOD 安装页，并高亮对应仓库卡片 3 秒。 */
const { linkToRepository } = useRepositoryLink()
/** 从 MOD 安装页跳回来时，高亮绑定到该仓库的 MOD 卡片 3 秒并滚动到它。 */
const { highlightId } = useFocusHighlight()

const sortBy = ref<'name' | 'version' | 'enabled'>('name')
const sortLabels = computed(() => ({ name: t('按名称'), version: t('按版本'), enabled: t('按启用') }))
function cycleSort() {
  const order = ['name', 'version', 'enabled'] as const
  sortBy.value = order[(order.indexOf(sortBy.value) + 1) % order.length]
}
const sortLabel = computed(() => sortLabels.value[sortBy.value])
const busy = ref<Record<string, boolean>>({})
const expanded = ref<Record<string, boolean>>({})

// 切页面复用已有 Dashboard；开关/删除后仍用 load() 取最新扫描结果
onMounted(() => { void ensureLoaded() })

const sortedMods = computed(() => {
  const list = [...mods.value]
  if (sortBy.value === 'name') list.sort((a, b) => modTitle(a).localeCompare(modTitle(b)))
  if (sortBy.value === 'version') {
    list.sort((a, b) => {
      const compared = compareVersions(b.version, a.version)
      return compared ?? (b.version ?? '').localeCompare(a.version ?? '')
    })
  }
  if (sortBy.value === 'enabled') list.sort((a, b) => Number(b.enabled) - Number(a.enabled))
  return list
})

const enabledCount = computed(() => mods.value.filter((mod) => mod.enabled).length)
const updateCount = computed(() => mods.value.filter((mod) => mod.update_available).length)

/**
 * 按「已安装时绑定的仓库分类」分组。
 * MOD 通过 repository_id / repository_binding 找到对应仓库，取其 group_i18n；
 * 找不到绑定的 MOD 归入「未绑定」分组。
 */
interface ModGroupNode {
  key: string
  label: string
  mods: InstalledMod[]
  count: number
  updateCount: number
  enabled: boolean
  partial: boolean
}

const repositories = computed<Repository[]>(() => config.value?.repositories ?? [])

function pickI18n(map: Record<string, string> | undefined, fallback: string): string {
  if (!map) return fallback
  return map[locale.value] || map['zh-CN'] || Object.values(map)[0] || fallback
}

/**
 * MOD 的分类：优先用安装清单里的分组快照（后端 `group_i18n`）。
 *
 * 分组语义是「安装时绑定的仓库分类」，快照在安装那一刻固化，
 * 所以刚启动、仓库源还没刷新完（config.repositories 可能为空或仍旧）时也不会乱。
 * 快照里没有分组才回退到当前仓库配置；找不到仓库归入「未绑定」。
 */
interface ModCategory { key: string; label: string }
function modCategory(mod: InstalledMod): ModCategory {
  const snapshot = mod.group_i18n
  const snapshotGroup = (mod.group || '').trim()
  if (snapshot && Object.keys(snapshot).length > 0) {
    const canonical = (snapshotGroup || snapshot['zh-CN'] || pickI18n(snapshot, '')).trim()
    const label = pickI18n(snapshot, snapshotGroup) || t('未分组')
    return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
  }
  if (snapshotGroup) {
    return { key: `group:${snapshotGroup.toLowerCase()}`, label: snapshotGroup }
  }

  const repo = repositoryForMod(repositories.value, mod)
  if (!repo) return { key: '__unbound__', label: t('未绑定') }
  // key 用语言无关的原始分组名，切换语言时折叠状态不丢
  const canonical = (repo.group || repo.group_i18n?.['zh-CN'] || pickI18n(repo.group_i18n, '')).trim()
  const label = pickI18n(repo.group_i18n, repo.group || '') || t('未分组')
  return { key: canonical ? `group:${canonical.toLowerCase()}` : '__ungrouped__', label }
}

const groups = computed<ModGroupNode[]>(() => {
  const buckets = new Map<string, { label: string; mods: InstalledMod[] }>()
  for (const mod of sortedMods.value) {
    const category = modCategory(mod)
    const bucket = buckets.get(category.key)
    if (bucket) bucket.mods.push(mod)
    else buckets.set(category.key, { label: category.label, mods: [mod] })
  }
  return [...buckets.entries()]
    .sort(([aKey, a], [bKey, b]) => {
      // 「未绑定」「未分组」排最后
      const tails = new Set(['__unbound__', '__ungrouped__'])
      if (tails.has(aKey) !== tails.has(bKey)) return tails.has(aKey) ? 1 : -1
      return a.label.localeCompare(b.label)
    })
    .map(([key, { label, mods: list }]) => {
      const enabled = list.filter((mod) => mod.enabled).length
      return {
        key,
        label,
        mods: list,
        count: list.length,
        updateCount: list.filter((mod) => mod.update_available).length,
        enabled: enabled === list.length,
        partial: enabled > 0 && enabled < list.length,
      }
    })
})

const allGroupKeys = computed(() => groups.value.map((node) => node.key))
/** 默认全部展开：新出现的分类自动展开，用户的折叠选择单独记住。 */
const collapsedGroups = ref<string[]>([])
const groupsExpanded = computed(() => allGroupKeys.value.filter((key) => !collapsedGroups.value.includes(key)))
function onGroupsExpanded(names: string[]) {
  const open = new Set(names)
  collapsedGroups.value = allGroupKeys.value.filter((key) => !open.has(key))
}

/** 卡片标题：优先 MOD 包名（同一包内的子 MOD 显示同一个包标题），没有包则用插件名。 */
function modTitle(mod: InstalledMod) {
  return mod.package_name?.trim() || mod.plugin_name?.trim() || mod.title || mod.product_name || mod.name
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
function isExpanded(dll: string) {
  return !!expanded.value[dll]
}
/** 点击卡片展开/收起详情（开关与删除按钮已阻止冒泡）。 */
function onCardClick(mod: InstalledMod) {
  toggleExpand(mod.dll_file)
}

/** 后端 labels → 文字 + 语义色（每个状态标签一种颜色，与 MOD 安装页共用）。 */
const modTags = (mod: InstalledMod) => installedModTags(mod).map((tag) => ({ ...tag, text: t(tag.text) }))
function statusSummary(mod: InstalledMod) {
  return modTags(mod).map((tag) => tag.text).join(' · ')
}

/** 该 MOD 绑定仓库的 id（优先安装清单，其次身份绑定），用于标签跳转。 */
function bindingRepositoryId(mod: InstalledMod): string | undefined {
  return repositoryForMod(repositories.value, mod)?.id
}

/** 点击状态标签：跳到 MOD 安装页并高亮对应仓库卡片。 */
function openInRepository(mod: InstalledMod) {
  const id = bindingRepositoryId(mod)
  if (id) linkToRepository(id)
}

/** 绑定仓库展示：provider + owner/repo（@tag）。 */
function bindingText(mod: InstalledMod) {
  const binding = mod.repository_binding
  if (!binding) return mod.repository_id ? `#${mod.repository_id}` : '—'
  const tag = binding.tag ? `@${binding.tag}` : ''
  return `${binding.provider}: ${binding.owner}/${binding.repository}${tag}`
}

/** 包成员数文案。 */
function packageCountText(mod: InstalledMod) {
  return t('{count} 个', { count: mod.package_mod_count ?? 0 })
}

/* ===== 工具栏：定位目录 / 配置编辑 ===== */

/** 配置编辑器弹窗（同一时刻只编辑一个 MOD）。 */
const configOpen = ref(false)
const configTarget = ref<InstalledMod | null>(null)

/**
 * 工具栏列宽：按「可选项占位 + 两个固定按钮」计算固定总宽。
 * 可选项（配置编辑）在最左侧，缺失时用同宽占位补上，
 * 这样「打开位置」「删除」永远落在最右同一列，图标竖排对齐。
 */
const TOOL_BUTTON_WIDTH = 30
const TOOL_GAP = 2
const toolColumnWidth = computed(() => 3 * TOOL_BUTTON_WIDTH + 2 * TOOL_GAP)

/** 打开该 MOD 所在文件夹并选中文件（后端只允许当前游戏的 BepInEx/plugins）。 */
async function openLocation(mod: InstalledMod) {
  try {
    const result = await api.openModFileLocation(mod.dll_file)
    if (!result.ok) message.error(result.message)
  } catch (reason) { fail(reason) }
}

/** 打开配置编辑器（仅对存在配置文件的 MOD 可用）。 */
function editConfig(mod: InstalledMod) {
  configTarget.value = mod
  configOpen.value = true
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

    <!-- 按安装时绑定仓库的分类折叠：同一分类的 MOD 归在一起 -->
    <n-collapse v-else :expanded-names="groupsExpanded" class="group-collapse"
      @update:expanded-names="onGroupsExpanded">
      <n-collapse-item v-for="node in groups" :key="node.key" :name="node.key">
        <template #header>
          <div class="group-head">
            <b>{{ node.label }}</b>
            <n-tag size="tiny" :bordered="false" class="state-tag state-tag--muted">{{ node.count }}</n-tag>
            <n-tag v-if="node.updateCount" size="tiny" :bordered="false" class="state-tag state-tag--warn">
              {{ t('{count} 个可更新', { count: node.updateCount }) }}
            </n-tag>
            <n-tag v-if="!node.enabled && !node.partial" size="tiny" :bordered="false"
              class="state-tag state-tag--muted">{{ t('已禁用') }}</n-tag>
            <n-tag v-else-if="node.partial" size="tiny" :bordered="false"
              class="state-tag state-tag--idle">{{ t('部分启用') }}</n-tag>
          </div>
        </template>

        <div class="mod-grid">
          <article v-for="mod in node.mods" :key="mod.dll_file" class="mod-card"
            :data-focus-id="bindingRepositoryId(mod)"
            :class="{
              disabled: !mod.enabled,
              expanded: isExpanded(mod.dll_file),
              'focus-flash': highlightId === bindingRepositoryId(mod),
            }" @click="onCardClick(mod)">
            <!-- 单行：左「开关 + 标题」… 右「标签 | 工具栏」 -->
            <div class="card-row">
              <div class="card-left">
                <n-switch class="card-switch" size="small" :value="mod.enabled" :loading="!!busy[mod.dll_file]"
                  @click.stop @update:value="(value: boolean) => toggle(mod, value)" />
                <b class="card-title" :title="modTitle(mod)">{{ modTitle(mod) }}</b>
              </div>

              <div class="card-right">
                <!-- 状态标签组：固定宽度按内容自适应，整体贴住分割线左侧 -->
                <span class="tag-group">
                  <n-tag v-for="tag in modTags(mod)" :key="tag.key" size="tiny" :bordered="false"
                    class="state-tag state-tag--link" :class="`state-tag--${tag.tone}`"
                    :title="bindingRepositoryId(mod) ? t('在 MOD 安装中查看') : undefined"
                    @click.stop="openInRepository(mod)">{{ tag.text }}</n-tag>
                </span>

                <!-- 分割线：把标签区和操作按钮区分开 -->
                <span class="tool-divider" />

                <!-- 工具栏：可选按钮在最左（用占位保持列宽），固定按钮贴右 -->
                <span class="tool-group" :style="{ minWidth: `${toolColumnWidth}px` }">
                  <!-- 配置编辑：只有存在配置文件的 MOD 才有；缺失时保留同宽占位，
                       这样「打开位置」「删除」始终落在最右侧同一列 -->
                  <n-button v-if="mod.has_config" size="tiny" quaternary :title="t('配置编辑')"
                    class="tool-optional" @click.stop="editConfig(mod)">
                    <SettingsOutline width="14" height="14" />
                  </n-button>
                  <span v-else class="tool-placeholder" />
                  <span class="tool-spacer" />
                  <n-button size="tiny" quaternary :title="t('打开所在文件夹并定位')"
                    :disabled="game?.kitchen_running" @click.stop="openLocation(mod)">
                    <FolderOpenOutline width="14" height="14" />
                  </n-button>
                  <n-button size="tiny" quaternary type="error" :disabled="game?.kitchen_running" :title="t('删除')"
                    @click.stop="remove(mod)">
                    <TrashOutline width="14" height="14" />
                  </n-button>
                </span>
              </div>
            </div>

            <!-- 展开：后端返回的完整信息 -->
            <div v-if="isExpanded(mod.dll_file)" class="card-detail" @click.stop>
              <div class="detail-grid">
                <div class="detail-item"><span class="muted">{{ t('名称') }}</span><b>{{ mod.name }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('标题') }}</span><b>{{ mod.title || '—' }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('产品名') }}</span><b>{{ mod.product_name || '—' }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('原始文件名') }}</span><b class="mono">{{ mod.original_filename || '—' }}</b></div>
                <div class="detail-item"><span class="muted">GUID</span><b class="mono">{{ mod.guid || '—' }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('插件名') }}</span><b>{{ mod.plugin_name || '—' }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('作者') }}</span><b>{{ mod.company || '—' }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('版权') }}</span><b>{{ mod.copyright || '—' }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('版本') }}</span><b class="mono">{{ mod.version || '—' }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('版本来源') }}</span><b>{{ mod.version_source || '—' }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('启用状态') }}</span><b>{{ mod.enabled ? t('已启用') : t('已禁用') }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('配置文件') }}</span><b>{{ mod.has_config ? t('有') : t('无') }}</b></div>
                <div class="detail-item"><span class="muted">{{ t('重复 GUID') }}</span><b>{{ mod.duplicate_guid ? t('是') : t('否') }}</b></div>
                <div class="detail-item wide"><span class="muted">{{ t('文件') }}</span><b class="mono">{{ mod.dll_file }}</b></div>
                <div class="detail-item wide"><span class="muted">{{ t('绑定仓库') }}</span><b class="mono">{{ bindingText(mod) }}</b></div>
                <div class="detail-item wide"><span class="muted">{{ t('状态标签') }}</span><b>{{ statusSummary(mod) || '—' }}</b></div>
                <div v-if="mod.package_name" class="detail-item wide">
                  <span class="muted">{{ t('MOD 包') }}</span>
                  <b>{{ mod.package_name }}（{{ packageCountText(mod) }}）</b>
                </div>
                <div v-if="mod.load_warning" class="detail-item wide">
                  <span class="muted">{{ t('加载警告') }}</span>
                  <b class="warn">{{ mod.load_warning }}</b>
                </div>
              </div>
            </div>
          </article>
        </div>
      </n-collapse-item>
    </n-collapse>
  </n-spin>

  <!-- 只保留排序切换（卡片已是唯一布局） -->
  <n-float-button position="fixed" bottom="28" right="32" :tooltip="`${t('排序')}：${sortLabel}`"
    @click="cycleSort">
    <SwapVerticalOutline width="17" height="17" />
  </n-float-button>

  <!-- 配置编辑器（Monaco）：编辑该 MOD 在 BepInEx/config 下的 .cfg -->
  <ModConfigEditor v-if="configTarget" v-model:show="configOpen" :dll-file="configTarget.dll_file"
    :title="modTitle(configTarget)" />
</template>

<style scoped>
.toolbar {
  display: flex; align-items: center; gap: 12px; margin-bottom: 14px;
}
.toolbar > .muted { margin-right: auto; }
.seg { display: flex; gap: 6px; }
.empty { margin: 60px 0; }
/* 分类折叠：分类标题行 + 内部单列 MOD 卡片 */
.group-collapse { width: 100%; }
.group-head {
  display: flex;
  align-items: center;
  gap: 9px;
  min-width: 0;
}
.group-head b {
  font-size: 14px;
  color: var(--oc2-accent);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mod-grid {
  /* 每个卡片撑满容器宽度（单列纵向堆叠） */
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 2px 0 6px 14px;
}
/* 卡片：单行三段 —— 左「开关 + 标题」，右「标签 + 删除」 */
.mod-card {
  display: flex;
  flex-direction: column;
  padding: 10px 14px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
  cursor: pointer;
  transition: border-color .16s ease, background-color .16s ease, box-shadow .16s ease;
}
.mod-card:hover {
  border-color: var(--oc2-accent);
  background: var(--oc2-row-hover);
  box-shadow: 0 4px 16px rgba(0, 0, 0, .16);
}
.mod-card.expanded {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 8%, var(--oc2-card));
}
.mod-card.disabled { opacity: .62; }
/* 跨页跳转高亮：3 秒后自动褪去（时长由 crossLink.ts 控制） */
.mod-card.focus-flash {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 16%, var(--oc2-card));
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent);
}
/* 可点击的状态标签：手型 + hover 反馈，表示能跳到 MOD 安装页 */
.state-tag--link { cursor: pointer; }
.state-tag--link:hover { filter: brightness(1.25); }
.card-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  min-width: 0;
}
.card-left {
  display: flex;
  align-items: center;
  gap: 11px;
  flex: 1;
  min-width: 0;
}
/*
 * 右侧一行三段：[状态标签组] | [分割线] | [工具栏]
 * 三段都垂直居中、不换行，工具栏宽度按全页最多按钮数固定，
 * 所以每个 MOD 的按钮列都对齐在同一条竖线上。
 */
.card-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: nowrap;
  gap: 10px;
  flex: 0 0 auto;
  min-width: 0;
}
/* 标签组：右对齐贴住分割线，标签之间 6px */
.tag-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
  min-width: 0;
  flex-wrap: nowrap;
  overflow: hidden;
}
/* 竖分割线：只做视觉分隔，不占额外空间 */
.tool-divider {
  width: 1px;
  height: 16px;
  flex: 0 0 1px;
  background: var(--oc2-row-line);
}
/* 工具栏：可选项在最左、固定按钮贴右，中间用 spacer 推开 */
.tool-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 2px;
  flex: 0 0 auto;
}
.tool-group :deep(.n-button) {
  width: 30px;
  min-width: 30px;
  padding: 0;
}
/* 没有配置按钮时保留同宽占位，保证右侧两个按钮列对齐 */
.tool-placeholder {
  width: 30px;
  height: 1px;
  flex: 0 0 30px;
}
/* 把可选项与固定按钮推成两端：可选项贴左，固定按钮贴右 */
.tool-spacer { flex: 1 1 auto; min-width: 2px; }
.card-switch { flex: 0 0 auto; }
.card-title {
  font-size: 13.5px;
  font-weight: 600;
  color: var(--oc2-text-1);
  line-height: 1.35;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 展开：后端返回的完整信息网格 */
.card-detail {
  margin-top: 10px;
  padding-top: 10px;
  border-top: 1px solid var(--oc2-row-line);
}
.detail-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 8px 16px;
}
.detail-item {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.detail-item.wide { grid-column: 1 / -1; }
.detail-item span { font-size: 11px; }
.detail-item b {
  font-size: 12px;
  color: var(--oc2-text-1);
  font-weight: 500;
  overflow-wrap: anywhere;
}
.detail-item b.warn { color: var(--warning, #e6b86a); }
</style>
