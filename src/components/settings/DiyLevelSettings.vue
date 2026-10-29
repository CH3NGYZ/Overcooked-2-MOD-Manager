<script setup lang="ts">
/**
 * DIYLevel 自定义关卡框架管理（设置页「路径管理」下的一个区块）。
 *
 * 为什么它单独有个入口：DIYLevel 本质是一个 MOD，但承担「框架」角色 ——
 * 自制地图靠它加载。让用户在 MOD 列表里自己找显然不合适，所以设置页给它
 * 一个固定位置，和 BepInEx 框架并列。
 *
 * 与另外两个概念严格区分：
 *   · BepInEx 是**加载器**（`bepinex::installed` 判核心程序集）
 *   · DIYLevel 是**内容框架 MOD**（GUID `dev.gua.overcooked.diylevel`）
 *   · 地图文件是用户数据，放在 `BepInEx/plugins/OC2DIYLevel/levels`
 *
 * 安装动作不自己实现下载：有在线仓库时直接复用仓库卡片的安装链路
 * （`/mods-online?focus=<仓库id>` 会定位并高亮那张卡片），没有仓库时
 * 如实告诉用户只能手动安装 —— 不伪造一个点了会失败的按钮。
 */
import { computed, onMounted, ref } from 'vue'
import { NButton, NSpin, NSwitch, NTag } from 'naive-ui'
import { FolderOpenOutline, DownloadOutline, RefreshOutline, TrashOutline } from '@vicons/ionicons5'
import { api } from '../../services/tauri'
import { useNotify } from '../../useNotify'
import { pickI18n, t } from '../../i18n'
import { tagColorFor } from '../../theme'
import { useDashboard } from '../../composables/useDashboardStore'
import { useModRemoval } from '../../composables/useModRemoval'
import { useRepositoryLink } from '../../crossLink'
import SettingsRow from './SettingsRow.vue'
import type { DiyLevelInfo, Repository } from '../../types'

/** DIYLevel 的稳定 GUID：与仓库源、安装清单、MOD 扫描用同一个标识。 */
const DIYLEVEL_GUID = 'dev.gua.overcooked.diylevel'

const message = useNotify()
const { load, game, config, mods } = useDashboard()
const { linkToRepository } = useRepositoryLink()

const info = ref<DiyLevelInfo | null>(null)
const loading = ref(false)
const switching = ref(false)

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

const installed = computed(() => info.value?.installed === true)
const enabled = computed(() => info.value?.enabled === true)

async function refresh() {
  loading.value = true
  try {
    info.value = await api.diyLevelInfo()
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
  } finally {
    loading.value = false
  }
}

/**
 * 安装 / 重装。
 *
 * 有候选仓库时**跳到 MOD 安装页并高亮那张卡片**，由那里走正常的
 * 下载预览 → 确认安装链路。这样 DIYLevel 的安装与任何 MOD 完全一致，
 * 不会出现「设置页有一套独立安装逻辑」这种分叉。
 */
function openInstall() {
  const repo = installableRepository.value
  if (!repo) {
    message.warning(t('无法在线安装 DIYLevel，请手动安装后在这里管理'))
    return
  }
  linkToRepository(repo.id)
}

/** 打开地图目录（后端只允许游戏内的固定子路径，不自动创建）。 */
async function openDirectory() {
  try {
    const result = await api.openDiyLevelDirectory()
    if (!result.ok) message.error(result.message)
  } catch (reason) {
    message.error(reason instanceof Error ? reason.message : String(reason))
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
    message.error(reason instanceof Error ? reason.message : String(reason))
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

/** 从 MOD 扫描结果里取出这个 GUID 的那一项，交给共用卸载流程。 */
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
  <SettingsRow :title="t('DIYLevel 框架')"
    :description="t('自定义关卡框架（MOD）；未安装时自制地图无法加载')">
    <template #right>
      <n-spin v-if="loading" size="small" />
      <template v-else>
        <!-- 状态：已装显示版本，未装显示提示 -->
        <n-tag v-if="installed" size="small" :bordered="false"
          :color="tagColorFor(enabled ? 'accent' : 'muted')">
          {{ enabled ? t('已启用') : t('已禁用') }}
        </n-tag>
        <n-tag v-else size="small" :bordered="false" :color="tagColorFor('muted')">
          {{ t('未安装') }}
        </n-tag>
        <span v-if="installed && info?.version" class="mono version-hint">{{ info.version }}</span>

        <n-switch v-if="installed" size="small" :value="enabled" :loading="switching"
          :disabled="!!game?.kitchen_running" @update:value="toggle" />

        <!-- 已安装：打开目录 / 重装 / 卸载 -->
        <template v-if="installed">
          <n-button size="small" secondary @click="openDirectory">
            <FolderOpenOutline width="14" height="14" />{{ t('打开目录') }}
          </n-button>
          <n-button size="small" secondary :disabled="!installableRepository" @click="openInstall">
            <RefreshOutline width="14" height="14" />{{ t('重装') }}
          </n-button>
          <n-button size="small" secondary type="error" :disabled="!!game?.kitchen_running"
            @click="uninstall">
            <TrashOutline width="14" height="14" />{{ t('卸载') }}
          </n-button>
        </template>

        <!-- 未安装：有仓库才能装；没有就如实说明 -->
        <template v-else>
          <n-button v-if="installableRepository" size="small" type="primary" @click="openInstall">
            <DownloadOutline width="14" height="14" />{{ t('安装') }}
          </n-button>
          <span v-else class="muted unreachable">
            {{ t('无法在线安装，请手动安装后在这里管理') }}
          </span>
        </template>
      </template>
    </template>
  </SettingsRow>

  <!-- 候选仓库提示：让人知道「安装」会跳到哪张卡 -->
  <SettingsRow v-if="!installed && installableRepository" :title="t('在线安装来源')"
    :description="t('点击「安装」会跳转到该仓库卡片，与在那里安装完全一致')">
    <template #right>
      <n-tag size="small" :bordered="false" :color="tagColorFor('accent')">{{ repositoryName }}</n-tag>
    </template>
  </SettingsRow>
</template>

<style scoped>
.version-hint {
  font-size: 12px;
  color: var(--oc2-text-3);
}
.unreachable {
  font-size: 12px;
  max-width: 320px;
  text-align: right;
}
</style>
