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
import { NButton, NSwitch } from 'naive-ui'
import {
  AddCircleOutline, CloudDownloadOutline, RefreshOutline, SettingsOutline, TrashOutline,
} from '@vicons/ionicons5'
import StatCard from '../components/StatCard.vue'
import RecycleBinDialog from '../components/RecycleBinDialog.vue'
import { useDashboard } from '../composables/useDashboardStore'
import { api } from '../services/tauri'
import { repositoryIsAvailable } from '../repositoryAvailability'
import { t } from '../i18n'
import { resolvedUpdateAvailable } from '../statusTags'
import { useNotify } from '../useNotify'
import { FOCUS_QUERY } from '../crossLink'
import type { ModRecycleEntry } from '../types'

const router = useRouter()
const message = useNotify()
const { ensureLoaded, load, loading, error, mods, game, config, frameworkOk } = useDashboard()

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

/* ===== 卡片 2：已安装 MOD（含可更新数量） ===== */

const installedMods = computed(() => mods.value)

/**
 * 可更新的 MOD。
 *
 * **必须用 `resolvedUpdateAvailable` 而不是 `mod.update_available`。**
 * 后者是后端扫盘时的快照；Release 缓存是之后异步刷新回来的。
 * MOD 管理页会用 `recomputeModUpdate` 按当前缓存实时重算，总览若只读快照
 * 就会出现「管理页显示可更新、总览说全部为最新版」这种自相矛盾。
 */
const updatable = computed(() => {
  const repos = config.value?.repositories ?? []
  return mods.value.filter((mod) =>
    !mod.is_dependency && resolvedUpdateAvailable(mod, repos))
})

/**
 * 卡片副标题：有更新时报「N 个可更新」，没有时只报总数。
 * 主数值永远是已安装总数 —— 这张卡回答的是「我装了多少」，
 * 可更新只是附加信息，不该反过来占据主位（否则没更新时显示 0 很空洞）。
 */
const installedSub = computed(() => {
  const total = installedMods.value.length
  if (!total) return t('尚未安装任何 MOD')
  const updates = updatable.value.length
  return updates
    ? t('共 {count} 个，其中 {updates} 个可更新', { count: total, updates })
    : t('共 {count} 个，全部为最新版', { count: total })
})

/**
 * 跳到 MOD 管理页。
 *
 *   · 有更新 → 定位到第一条可更新的 MOD（复用包外壳的 `data-focus-id`）
 *   · 无更新 → 直接进管理页，不带定位
 */
function openLocalMods() {
  const first = updatable.value[0]
  if (!first) {
    void router.push({ name: 'mods-local' })
    return
  }
  const anchor = first.package_id || first.repository_id || first.dll_file
  void router.push({ name: 'mods-local', query: { [FOCUS_QUERY]: anchor } })
}

/* ===== 卡片 3：在线 MOD（含刷新失败提示） ===== */

/**
 * 当前地区可见的仓库 —— 口径与 MOD 安装页的「共 N 个仓库」一致
 * （都用 `repositoryIsAvailable`），只是这里不带搜索过滤。
 */
const onlineRepositories = computed(() =>
  (config.value?.repositories ?? []).filter(
    (repo) => repositoryIsAvailable(repo, config.value?.repository_sources, config.value?.region),
  ))

/** 其中刷新报错的那些。隐藏的仓库不算，它们本来就不参与刷新。 */
const failedRepositories = computed(() =>
  onlineRepositories.value.filter((repo) => repo.last_error))

/**
 * 卡片副标题：主数值是在线仓库总数；有失败时追加「N 个刷新失败（仓库名）」。
 *
 * 把失败信息塞进这张卡而不是单开一张：失败仓库本身就是在线仓库的一部分，
 * 拆成两张会让「26 个仓库」和「1 个失败」看起来是两件事。
 */
const onlineSub = computed(() => {
  const total = onlineRepositories.value.length
  if (!total) return t('尚未加载任何仓库')
  if (!failedRepositories.value.length) {
    return t('共 {count} 个仓库 · 全部正常', { count: total })
  }
  const names = failedRepositories.value.slice(0, 2).map((repo) => repo.name).join('、')
  const more = failedRepositories.value.length > 2
    ? t('等 {count} 个', { count: failedRepositories.value.length })
    : ''
  return t('共 {count} 个仓库 · {failed} 个刷新失败（{names}）', {
    count: total,
    failed: failedRepositories.value.length,
    names: `${names}${more}`,
  })
})

/**
 * 卡片按钮随状态分支：
 *   · 有仓库刷新失败 → 「去刷新」，跳到安装页并高亮第一个失败仓库
 *   · 全部正常 → 「去安装」，只跳转到安装页
 */
function openOnlineRepositories() {
  const first = failedRepositories.value[0]
  if (!first) {
    void router.push({ name: 'mods-online' })
    return
  }
  void router.push({ name: 'mods-online', query: { [FOCUS_QUERY]: first.id } })
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
</script>

<template>
  <div class="overview">
    <div v-if="error" class="muted">{{ error }}</div>
    <div v-else class="card-grid" :class="{ 'is-loading': loading }">
      <!--
        1. BepInEx：整张卡可点，跳设置页并自动展开框架抽屉。
           开关单独放在右侧，点开关不会触发跳转（@click.stop）。
      -->
      <StatCard :label="t('BepInEx 框架')" :value="frameworkValue"
        :sub="frameworkSub" :tone="frameworkOk ? 'success' : 'warning'"
        clickable @click="openFrameworkSettings">
        <template #action>
          <n-switch v-if="frameworkOk" size="small" :value="frameworkEnabled"
            :disabled="working" :title="t('框架启用（游戏加载 MOD）')"
            @click.stop @update:value="toggleFramework" />
          <n-button v-else size="small" type="primary" @click.stop="openFrameworkSettings">
            <template #icon><AddCircleOutline width="14" height="14" /></template>
            {{ t('安装框架') }}
          </n-button>
        </template>
      </StatCard>

      <!--
        2. 已安装 MOD：主数值是安装总数，旁边并列「N 可更新」。
           两个数字各用各的等级色（总数=中性/正常，可更新=中度），
           这样一眼能看出「4 个里有 1 个要处理」，不会把两个数读成一个。
           按钮随状态分支：有更新 →「去更新」并定位第一条；无更新 →「去管理」。
      -->
      <StatCard :label="t('已安装 MOD')" :value="String(installedMods.length)"
        :sub="installedSub"
        :tone="updatable.length ? 'warning' : 'success'">
        <template v-if="updatable.length" #extra>
          <span class="extra-value update-color">{{ updatable.length }}</span>
          <span class="extra-label update-color">{{ t('可更新') }}</span>
        </template>
        <template #action>
          <n-button size="small" :secondary="!updatable.length"
            :type="updatable.length ? 'warning' : 'default'"
            :disabled="!installedMods.length" @click="openLocalMods">
            <template #icon>
              <CloudDownloadOutline v-if="updatable.length" width="14" height="14" />
              <SettingsOutline v-else width="14" height="14" />
            </template>
            {{ updatable.length ? t('去更新') : t('去管理') }}
          </n-button>
        </template>
      </StatCard>

      <!--
        3. 在线 MOD：主数值是可见仓库总数，旁边并列「N 刷新失败」。
           有失败时按钮「去刷新」并高亮第一个失败仓库；全部正常时「去安装」。
      -->
      <StatCard :label="t('在线 MOD')" :value="String(onlineRepositories.length)"
        :sub="onlineSub"
        :tone="failedRepositories.length ? 'warning' : 'success'">
        <template v-if="failedRepositories.length" #extra>
          <span class="extra-value refresh-color">{{ failedRepositories.length }}</span>
          <span class="extra-label refresh-color">{{ t('刷新失败') }}</span>
        </template>
        <template #action>
          <n-button size="small" :secondary="!failedRepositories.length"
            :type="failedRepositories.length ? 'warning' : 'default'"
            :disabled="!onlineRepositories.length" @click="openOnlineRepositories">
            <template #icon>
              <RefreshOutline v-if="failedRepositories.length" width="14" height="14" />
              <CloudDownloadOutline v-else width="14" height="14" />
            </template>
            {{ failedRepositories.length ? t('去刷新') : t('去安装') }}
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

/*
 * 并列的次要计数：数字比主数值小一档，下面跟一行小标签。
 *
 * 颜色不取 `--oc2-tone-*`：现有色阶里 moderate 与 severe 的 textColor
 * 都是同一个对比色（只有标签底色透明度不同），两个数字会完全同色，
 * 无法区分「可更新」和「刷新失败」。这里用两个独立的语义色，
 * 与全站色阶的分工不冲突 —— 色阶表达严重程度，这两个表达「哪一类问题」。
 */
.extra-value {
  font-size: 22px;
  font-weight: 700;
  line-height: 1.1;
}
.extra-label {
  font-size: 11.5px;
  font-weight: 600;
  line-height: 1;
}
/* 可更新：琥珀色，与「有动作但可等」的语义一致 */
.update-color { color: #f0b020; }
/* 刷新失败：蓝色，与「信息缺失、需要重新拉取」的语义一致 */
.refresh-color { color: #4da3ff; }

/*
 * 窄窗口（含 150% 内容缩放下等效宽度变小）时四列会挤，
 * 退成两列而不是让卡片文字被压成多行。
 */
@media (max-width: 1180px) {
  .card-grid { grid-template-columns: repeat(2, minmax(0, 1fr)); }
}
</style>
