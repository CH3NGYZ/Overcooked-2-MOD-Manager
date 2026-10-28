<script setup lang="ts">
/*
 * 总览页：四张卡片，每张回答一个「现在要不要管」的问题。
 *
 *   ｜BepInEx  ｜ 装没装、开没开 —— 没有它任何 MOD 都不生效
 *   ｜可更新   ｜ 有几项能升级 —— 点进去定位到第一条
 *   ｜仓库刷新 ｜ 有没有源挂了 —— 挂了的仓库拿不到版本信息
 *   ｜回收站   ｜ 攒了多少垃圾 —— 平时看不见、会一直涨
 *
 * 刻意不做的东西：趋势折线（没有历史存储）、状态饼图（十几项画饼图没信息量）、
 * 操作时间线（没有审计日志）。这页是「装完就走」的控制台，不是监控面板。
 *
 * 操作一律走「跳转到对应页面的定位位置」，不在这里直接执行：
 * 更新要看预览、依赖和被覆盖的文件，刷新失败要看仓库的错误详情，
 * 在总览里一键执行等于绕过这些信息。
 */
import { computed, onMounted, ref } from 'vue'
import { useRouter } from 'vue-router'
import { NButton, NSwitch, NTag, useDialog } from 'naive-ui'
import {
  AddCircleOutline, CloudDownloadOutline, RefreshOutline, TrashOutline, WarningOutline,
} from '@vicons/ionicons5'
import StatCard from '../components/StatCard.vue'
import RecycleBinDialog from '../components/RecycleBinDialog.vue'
import { useDashboard } from '../composables/useDashboardStore'
import { useRepositoryRefresh } from '../composables/useRepositoryRefresh'
import { api } from '../services/tauri'
import { repositoryIsAvailable } from '../repositoryAvailability'
import { t } from '../i18n'
import { tagColorFor } from '../theme'
import { useNotify } from '../useNotify'
import { FOCUS_QUERY } from '../crossLink'
import type { ModRecycleEntry } from '../types'

const router = useRouter()
const dialog = useDialog()
const message = useNotify()
const { ensureLoaded, load, loading, error, mods, game, config, frameworkOk } = useDashboard()
const { anyRefreshing, refreshEverything } = useRepositoryRefresh()

const recycleOpen = ref(false)
const recycleEntries = ref<ModRecycleEntry[]>([])

onMounted(() => {
  void ensureLoaded()
  void loadRecycle()
})

/** 回收站条目：只用来在卡片上显示有多少待清理。 */
async function loadRecycle() {
  try {
    recycleEntries.value = await api.getModRecycleBin()
  } catch {
    // 读取失败不该让整个总览报错：卡片退化成「—」即可
    recycleEntries.value = []
  }
}

/* ===== 卡片 1：BepInEx ===== */

const frameworkVersion = computed(() => game.value?.bepinex.version ?? '')
const frameworkEnabled = computed(() => game.value?.winhttp_state === 'modenabled')
/** 框架的完整状态文案：未装 / 已装未启用 / 正常。 */
const frameworkValue = computed(() => {
  if (!frameworkOk.value) return t('未安装')
  return frameworkEnabled.value ? t('已启用') : t('已禁用')
})
const frameworkSub = computed(() => {
  if (!frameworkOk.value) {
    return game.value?.bepinex.has_residue
      ? t('检测到 MOD 目录，安装后会保留')
      : t('安装后才能加载 MOD')
  }
  return frameworkVersion.value ? `BepInEx ${frameworkVersion.value}` : t('版本未知')
})

const working = ref(false)

/** 切换框架启用状态（只改 winhttp.dll 的名字，不碰 MOD 文件）。 */
async function toggleFramework(enabled: boolean) {
  if (working.value) return
  working.value = true
  try {
    const result = await api.switchWinhttp(enabled)
    if (result.ok) {
      message.success(result.message || (enabled ? t('框架已启用') : t('框架已禁用')))
      await load()
    } else {
      message.error(result.message)
    }
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    working.value = false
  }
}

/** 未安装时跳转到设置页的框架管理抽屉。 */
function openFrameworkSettings() {
  void router.push({ name: 'settings', query: { [FOCUS_QUERY]: 'bepinex' } })
}

/* ===== 卡片 2：可更新 ===== */

const updatable = computed(() =>
  mods.value.filter((mod) => mod.update_available && !mod.is_dependency))

/**
 * 跳到 MOD 管理页并定位到第一条可更新的 MOD。
 *
 * 定位用仓库 ID：MOD 管理页的包外壳带 `data-focus-id="pkg.<仓库ID>"`，
 * 与「最新版」标签的跳转共用同一套锚点，不额外造标记。
 */
function openFirstUpdate() {
  const first = updatable.value[0]
  const anchor = first?.package_id || first?.repository_id || first?.dll_file
  if (!anchor) return
  void router.push({ name: 'mods-local', query: { [FOCUS_QUERY]: anchor } })
}

/* ===== 卡片 3：仓库刷新失败 ===== */

/** 当前地区可见、且刷新报错的仓库。隐藏的仓库不该在这里出现。 */
const failedRepositories = computed(() =>
  (config.value?.repositories ?? []).filter(
    (repo) => repo.last_error && repositoryIsAvailable(repo, config.value?.repository_sources, config.value?.region),
  ))

/** 刷新全部仓库：失败项会重新尝试，成功后卡片自动变化。 */
async function refreshRepositories() {
  if (anyRefreshing.value) return
  await refreshEverything()
}

/* ===== 卡片 4：回收站 ===== */

const recycleCount = computed(() => recycleEntries.value.length)
/** 回收站里累计的文件数：比条目数更能反映「攒了多少东西」。 */
const recycleFiles = computed(() =>
  recycleEntries.value.reduce((total, entry) => total + (entry.file_count ?? 0), 0))

const recycleSub = computed(() => {
  if (!recycleCount.value) return t('暂无已删除的 MOD')
  return t('{count} 个已删除的 MOD，共 {files} 个文件', {
    count: recycleCount.value,
    files: recycleFiles.value,
  })
})

/** 打开回收站后重新统计：用户可能在里面彻底删除了几条。 */
async function closeRecycle() {
  recycleOpen.value = false
  await loadRecycle()
}

/* ===== 全部启用 / 全部禁用 ===== */

const enabledCount = computed(() => mods.value.filter((mod) => mod.enabled).length)
const allEnabled = computed(() => mods.value.length > 0 && enabledCount.value === mods.value.length)

/**
 * 批量开关全部 MOD。
 *
 * 禁用是「一次性关掉所有」——影响可逆、不发确认。
 * 启用同理。后端会整体成功或整体回滚，不会留下半开半关的状态。
 */
async function setAllEnabled(enabled: boolean) {
  if (working.value || !mods.value.length) return
  working.value = true
  try {
    const result = await api.setAllLocalModsEnabled(enabled)
    if (result.ok) {
      message.success(result.message)
      await load()
    } else {
      message.error(result.message)
    }
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    working.value = false
  }
}

function confirmDisableAll() {
  dialog.warning({
    title: t('禁用全部 MOD'),
    content: t('将禁用全部 {count} 个 MOD，游戏将不加载任何 MOD。可以随时重新启用。',
      { count: mods.value.length }),
    positiveText: t('全部禁用'),
    negativeText: t('取消'),
    onPositiveClick() {
      void setAllEnabled(false)
    },
  })
}
</script>

<template>
  <div class="overview">
    <div v-if="error" class="muted">{{ error }}</div>
    <div v-else class="card-grid" :class="{ 'is-loading': loading }">
      <!-- 1. BepInEx：开关直接作用于加载器，不需要跳转 -->
      <StatCard :label="t('BepInEx 框架')" :value="frameworkValue"
        :sub="frameworkSub" :tone="frameworkOk ? 'success' : 'warning'">
        <template #action>
          <n-switch v-if="frameworkOk" size="small" :value="frameworkEnabled"
            :disabled="working" :title="t('框架启用（游戏加载 MOD）')"
            @update:value="toggleFramework" />
          <n-button v-else size="small" type="primary" @click="openFrameworkSettings">
            <template #icon><AddCircleOutline width="14" height="14" /></template>
            {{ t('安装框架') }}
          </n-button>
        </template>
      </StatCard>

      <!-- 2. 可更新：主操作是跳过去看，不是在这里装 -->
      <StatCard :label="t('可更新 MOD')" :value="String(updatable.length)"
        :sub="updatable.length ? t('点击查看第一个可更新项') : t('全部为最新版')"
        :tone="updatable.length ? 'warning' : 'success'">
        <template #action>
          <n-button size="small" secondary :disabled="!updatable.length" @click="openFirstUpdate">
            <template #icon><CloudDownloadOutline width="14" height="14" /></template>
            {{ t('去更新') }}
          </n-button>
        </template>
      </StatCard>

      <!-- 3. 仓库刷新失败：就地重试，失败详情在 MOD 安装页 -->
      <StatCard :label="t('仓库刷新失败')" :value="String(failedRepositories.length)"
        :sub="failedRepositories.length
          ? failedRepositories.slice(0, 2).map((repo) => repo.name).join('、')
          : t('全部仓库刷新正常')"
        :tone="failedRepositories.length ? 'danger' : 'success'">
        <template #action>
          <n-button size="small" secondary :loading="anyRefreshing"
            :disabled="anyRefreshing" @click="refreshRepositories">
            <template #icon><RefreshOutline width="14" height="14" /></template>
            {{ t('刷新全部') }}
          </n-button>
        </template>
      </StatCard>

      <!-- 4. 回收站：唯一需要「主动清理」的累积项 -->
      <StatCard :label="t('回收站')" :value="String(recycleCount)"
        :sub="recycleSub" :tone="recycleCount ? 'warning' : 'default'">
        <template #action>
          <n-button size="small" secondary :disabled="!recycleCount" @click="recycleOpen = true">
            <template #icon><TrashOutline width="14" height="14" /></template>
            {{ t('打开回收站') }}
          </n-button>
        </template>
      </StatCard>
    </div>

    <!-- 全部启用/禁用：跨全部 MOD 的操作，单独一行放，不挤占四张卡 -->
    <div v-if="mods.length" class="bulk-row">
      <div class="bulk-text">
        <b>{{ t('全部 MOD') }}</b>
        <span class="muted">
          {{ t('已启用 {enabled} / {total}', { enabled: enabledCount, total: mods.length }) }}
        </span>
      </div>
      <n-tag v-if="!allEnabled" size="small" :bordered="false" :color="tagColorFor('mild')">
        <template #icon><WarningOutline width="12" height="12" /></template>
        {{ t('有 MOD 未启用') }}
      </n-tag>
      <!--
        两个动作始终都给：部分启用时用户可能想「一键全开」也可能想「干脆全关」。
        只显示一个会让另一种意图无路可走。
      -->
      <div class="bulk-actions">
        <n-button size="small" secondary :disabled="working || allEnabled"
          @click="setAllEnabled(true)">
          {{ t('全部启用') }}
        </n-button>
        <n-button size="small" secondary type="warning" :disabled="working || !enabledCount"
          @click="confirmDisableAll">
          {{ t('全部禁用') }}
        </n-button>
      </div>
    </div>

    <RecycleBinDialog :show="recycleOpen"
      @update:show="(value: boolean) => { if (!value) void closeRecycle() }" />
  </div>
</template>

<style scoped>
.overview {
  display: flex;
  flex-direction: column;
  gap: 16px;
  /* 数据未到时不清空布局，避免卡片塌缩后再撑开 */
  min-height: 0;
}
.card-grid {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 16px;
}
.card-grid.is-loading { opacity: .6; }

.bulk-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  border-radius: 12px;
  background: var(--oc2-sunken);
  border: 1px solid var(--oc2-row-line);
}
.bulk-text {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}
.bulk-text b { font-size: 13.5px; }
.bulk-text .muted { font-size: 12.5px; }
.bulk-actions { margin-left: auto; display: flex; gap: 8px; }

/*
 * 窄窗口（含 150% 内容缩放下等效宽度变小）时四列会挤，
 * 退成两列而不是让卡片文字被压成多行。
 */
@media (max-width: 1180px) {
  .card-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
