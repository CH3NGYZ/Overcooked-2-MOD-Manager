<script setup lang="ts">
/**
 * DIYLevel 框架管理面板（放在设置页右侧抽屉里，与 BepInExPanel 同形态）。
 *
 * 为什么和 BepInEx 对齐：两者都是「框架」，用户对它们的心智模型一样 ——
 * 一行摘要看状态，点进去才有安装/开关/路径/卸载。之前 DIYLevel 是内联
 * 整块卡片、BepInEx 是抽屉，同一类东西两套交互，用户不知道该去哪儿找。
 *
 * DIYLevel 本质是一个 MOD（GUID `dev.gua.overcooked.diylevel`），但承担
 * 「框架」角色 —— 自制地图靠它加载。与另外两个概念严格区分：
 *   · BepInEx 是**加载器**（`bepinex::installed` 判核心程序集）
 *   · DIYLevel 是**内容框架 MOD**
 *   · 地图文件是用户数据，放在 `BepInEx/plugins/OC2DIYLevel/levels`
 *
 * 安装动作不自己实现下载：有在线仓库时直接跳到仓库卡片
 * （`/mods-online?focus=<仓库id>` 定位并高亮），没有仓库时如实说明只能手动装。
 */
import { computed, onMounted, ref } from 'vue'
import { NButton, NSpin, NSwitch, NTag } from 'naive-ui'
import {
  AddCircleOutline, FolderOpenOutline, RefreshOutline, TrashOutline,
} from '@vicons/ionicons5'
import { api } from '../../services/tauri'
import { useNotify } from '../../useNotify'
import { pickI18n, t } from '../../i18n'
import { tagColorFor } from '../../theme'
import { useDashboard } from '../../composables/useDashboardStore'
import { useModRemoval } from '../../composables/useModRemoval'
import { useRepositoryLink } from '../../crossLink'
import type { DiyLevelInfo, Repository } from '../../types'

/** DIYLevel 的稳定 GUID：与仓库源、安装清单、MOD 扫描用同一个标识。 */
const DIYLEVEL_GUID = 'dev.gua.overcooked.diylevel'

const message = useNotify()
const { load, loading, game, config, mods } = useDashboard()
const { linkToRepository } = useRepositoryLink()

const info = ref<DiyLevelInfo | null>(null)
const refreshing = ref(false)
const switching = ref(false)

const installed = computed(() => info.value?.installed === true)
const enabled = computed(() => info.value?.enabled === true)
const levelPacks = computed(() => info.value?.level_packs ?? 0)
/** 游戏运行时禁止改名/删除，与 BepInEx 面板同一约束。 */
const busy = computed(() => switching.value || !!game.value?.kitchen_running)

const repositories = computed<Repository[]>(() => config.value?.repositories ?? [])

/**
 * 能在线上提供 DIYLevel 的仓库（按 GUID 匹配）。
 *
 * 与依赖解析的「可安装来源」同一判据：必须声明同一 GUID、是 Release 模式、
 * 且已有缓存的 Release —— 三者缺一，点了也只有下载失败。
 */
const installableRepository = computed<Repository | undefined>(() =>
  repositories.value.find((repo) =>
    !!repo.plugin_guid
    && repo.plugin_guid.trim().toLowerCase() === DIYLEVEL_GUID
    && repo.download_mode === 'release'
    && (repo.cached_releases?.length ?? 0) > 0),
)

const repositoryName = computed(() =>
  installableRepository.value
    ? pickI18n(installableRepository.value.name_i18n, installableRepository.value.name)
    : '')

function fail(reason: unknown) {
  message.error(reason instanceof Error ? reason.message : String(reason))
}

async function refresh() {
  refreshing.value = true
  try {
    info.value = await api.diyLevelInfo()
  } catch (reason) {
    fail(reason)
  } finally {
    refreshing.value = false
  }
}

/** 安装 / 重装：跳到 MOD 安装页并高亮对应仓库卡片，走同一条安装链路。 */
function openInstall() {
  const repo = installableRepository.value
  if (!repo) {
    message.warning(t('无法在线安装 DIYLevel，请手动安装后在这里管理'))
    return
  }
  linkToRepository(repo.id)
}

async function openDirectory() {
  try {
    const result = await api.openDiyLevelDirectory()
    if (!result.ok) message.error(result.message)
  } catch (reason) {
    fail(reason)
  }
}

/** 启用 / 禁用：走后端与 MOD 列表相同的 .dll 改名逻辑。 */
async function toggle(value: boolean) {
  if (switching.value) return
  switching.value = true
  try {
    const result = await api.setDiyLevelEnabled(value)
    result.ok ? message.success(result.message) : message.error(result.message)
    await refresh()
    // MOD 列表里也显示这个插件，状态变了要同步刷新
    await load()
  } catch (reason) {
    fail(reason)
  } finally {
    switching.value = false
  }
}

/** 卸载走与 MOD 列表共用的回收站流程。 */
const { remove } = useModRemoval({
  deleteCopy: () => (config.value?.recycle_deleted_mods !== false
    ? {
        body: (name: string) => t('「{name}」将被移入回收站，可在回收站中恢复。继续？', { name }),
        action: t('移入回收站'),
        done: '已移入回收站',
      }
    : {
        body: (name: string) => t('「{name}」将被直接删除，无法恢复。继续？', { name }),
        action: t('直接删除'),
        done: '已删除',
      }),
  titleOf: () => t('DIYLevel 框架'),
  onDone: async () => { await refresh(); await load() },
})

function uninstall() {
  const target = mods.value.find((mod) =>
    !!mod.guid && mod.guid.trim().toLowerCase() === DIYLEVEL_GUID)
  if (!target) {
    message.error(t('尚未安装 DIYLevel 框架'))
    return
  }
  remove(target, {
    title: t('卸载 DIYLevel 框架'),
    body: (name) => t('「{name}」将被卸载。其中的自制地图文件不会随框架一起删除。继续？',
      { name }),
  })
}

onMounted(() => { void refresh() })
</script>

<template>
  <n-spin :show="loading || refreshing">
    <!-- 状态摘要：开关 + 标题 + 版本 + 安装状态，与 BepInEx 面板同布局 -->
    <div class="panel-head">
      <div class="panel-head-main">
        <!-- 开关只在框架真正安装后才出现：没有可切换的插件 -->
        <n-switch v-if="installed" :value="enabled" :title="t('框架启用（游戏加载此框架）')"
          :loading="switching" :disabled="busy" @update:value="toggle" />
        <span class="panel-title">{{ t('DIYLevel 框架') }}</span>
        <span class="panel-version muted">{{ t('版本') }}：{{ info?.version || '—' }}</span>
        <n-tag size="small" :bordered="false" class="state-tag"
          :color="tagColorFor(installed ? (enabled ? 'accent' : 'muted') : 'muted')">
          {{ installed ? (enabled ? t('已启用') : t('已禁用')) : t('未安装') }}
        </n-tag>
      </div>
    </div>

    <!--
      框架自身的两个操作，与 BepInEx 面板一致：
      「打开目录」指向地图目录，「配置管理」在这里换成「地图数量」这类只读信息
      —— DIYLevel 没有 BepInEx 那样的 .cfg 配置面板。
    -->
    <div v-if="installed" class="panel-actions">
      <n-button size="small" secondary :disabled="busy" @click="openDirectory">
        <template #icon><FolderOpenOutline width="15" height="15" /></template>
        {{ t('打开地图目录') }}
      </n-button>
      <n-tag size="small" :bordered="false" type="default" class="panel-metric">
        {{ t('地图集') }}：{{ levelPacks }}
      </n-tag>
    </div>

    <!-- 安装：未安装时只有这一个入口；已安装则是「重新安装」+ 卸载 -->
    <div class="panel-install">
      <div class="panel-field">
        <span class="muted">{{ t('安装来源') }}</span>
        <span v-if="installableRepository" class="muted">{{ repositoryName }}</span>
        <span v-else class="muted">{{ t('未找到可在线安装的仓库') }}</span>
      </div>
      <div class="panel-buttons">
        <n-button size="small" type="primary" :disabled="busy || !installableRepository"
          @click="openInstall">
          <template #icon><AddCircleOutline width="15" height="15" /></template>
          {{ installed ? t('重新安装') : t('安装框架') }}
        </n-button>
        <template v-if="installed">
          <n-button size="small" secondary :disabled="busy" @click="openDirectory">
            <template #icon><RefreshOutline width="15" height="15" /></template>
            {{ t('刷新地图数量') }}
          </n-button>
          <n-button size="small" secondary type="error" :disabled="busy" @click="uninstall">
            <template #icon><TrashOutline width="15" height="15" /></template>
            {{ t('卸载') }}
          </n-button>
        </template>
      </div>
      <!-- 未安装时的说明：讲清装完之后能做什么，避免用户找不到维护入口 -->
      <p v-if="!installed" class="muted panel-hint">
        {{ installableRepository
          ? t('安装后这里会出现启用开关、地图目录与地图数量。')
          : t('无法在线安装：MOD 安装页没有声明此 GUID 的仓库，请手动安装后回到这里管理。') }}
      </p>
    </div>

    <!-- 地图集说明：数量来自 levels/ 下的文件夹 -->
    <div v-if="installed" class="panel-backups">
      <div class="panel-backups-head">
        <b>{{ t('地图集') }}</b>
        <n-button size="tiny" quaternary :disabled="busy" @click="openDirectory">
          <template #icon><FolderOpenOutline width="14" height="14" /></template>
          {{ t('打开 levels 目录') }}
        </n-button>
      </div>
      <div v-if="!levelPacks" class="muted panel-empty">
        {{ t('还没有地图集。把地图文件夹放进 levels 目录即可，每个文件夹算一个地图集。') }}
      </div>
      <div v-else class="muted panel-empty">
        {{ t('当前有 {count} 个地图集（每张地图集是 levels 下的一个文件夹）。', { count: levelPacks }) }}
      </div>
    </div>
  </n-spin>
</template>

<style scoped>
/*
 * 与 BepInExPanel 共用同一套 .panel-* 类名和观感：两个框架抽屉必须长得一样，
 * 否则用户会以为它们属于不同类型的对象。
 */
.panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  flex-wrap: wrap;
  padding-bottom: 12px;
  border-bottom: 1px solid var(--oc2-row-line);
}
.panel-head-main { display: flex; align-items: center; gap: 10px; flex-wrap: wrap; }
.panel-title { font-size: 14px; font-weight: 600; color: var(--oc2-text-1); }
.panel-version { font-size: 12px; }
.state-tag { font-size: 11px; }
.panel-metric { font-size: 11px; }

.panel-actions { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; margin-top: 12px; }
.panel-install { margin-top: 14px; display: flex; flex-direction: column; gap: 10px; }
.panel-field { display: flex; align-items: center; gap: 8px; font-size: 12px; }
.panel-buttons { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.panel-hint { font-size: 12px; margin: 0; line-height: 1.5; }

.panel-backups {
  margin-top: 18px;
  padding-top: 14px;
  border-top: 1px solid var(--oc2-row-line);
}
.panel-backups-head { display: flex; align-items: center; justify-content: space-between; gap: 8px; }
.panel-empty { font-size: 12px; margin-top: 8px; line-height: 1.5; }
</style>
