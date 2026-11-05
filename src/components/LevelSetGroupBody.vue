<script setup lang="ts">
import { computed, inject } from 'vue'
import { NButton, NTag } from 'naive-ui'
import { AlertCircleOutline, ArrowUpCircleOutline, DownloadOutline, FolderOpenOutline, TrashOutline } from '@vicons/ionicons5'
import { t, locale } from '../i18n'
import { formatVersion } from '../modStatus'
import { translateBackendMessage } from '../locale/backend'
import { buttonColorFor, tagColorForAny } from '../theme'
import { levelSetDisplayName, type LevelSetCardAction } from '../levelSetStatus'
import LocalItemCard from './LocalItemCard.vue'
import DependencyTable from './DependencyTable.vue'
import { LEVEL_SET_GROUP_CTX, TOOL_BUTTON_WIDTH, TOOL_GAP } from './levelSetGroupContext'
import type { LevelSetInfo } from '../types'

/**
 * 一个分组里的地图卡片（`LevelsView` 折叠项的内容区）。
 *
 * 为什么不写在页面的插槽里：`n-collapse` 的 vnode 只有 `expanded-names` 一个
 * 动态 prop，同组删掉一张卡时组集合没变、`expandedNames` 拿到同一个数组引用，
 * Vue 会判定整棵子树无需更新 —— 插槽闭包不重跑，组内 `v-for` 就停在旧数组上
 * （删空一个组才会因为组键变化而更新）。组件实例有自己的渲染 effect，父级被
 * 跳过也照样按上下文数据重渲，所以组内列表必须住在这里。
 *
 * 上下文里的值都是 ref/computed（页面用 `reactive()` provide），模板里读
 * `ctx.groups` / `ctx.busy[...]` 才能跟上页面数据。
 */
const props = defineProps<{ groupKey: string }>()

const ctx = inject(LEVEL_SET_GROUP_CTX)!

/** 本组的地图：键是永不变化的组键，数据变了这里自己重算。 */
const sets = computed(
  () => ctx.groups.find((group) => group.key === props.groupKey)?.sets ?? [],
)

/** 图标按动作类型取；模板里据此渲染唯一那颗按钮。 */
const ACTION_ICONS: Record<LevelSetCardAction['kind'], typeof DownloadOutline> = {
  update: ArrowUpCircleOutline,
  'install-dependency': DownloadOutline,
  'review-conflict': AlertCircleOutline,
}

/**
 * 一张卡最多一个动作，这里用数组包一层是为了让模板只调用一次
 * `ctx.actionOf` 并顺带拿到类型收窄（`v-if="ctx.actionOf(set)"` 不会把
 * 后面的 `action.tone` 变窄成非空）。
 */
function actionsOf(set: LevelSetInfo): LevelSetCardAction[] {
  const action = ctx.actionOf(set)
  return action ? [action] : []
}
</script>

<template>
  <div class="map-grid">
    <!--
      卡片 key 只用目录名。把 enabled 写进 key 会连整张卡片一起重建，
      开关每次都是「新挂载」，Naive 的切换过渡动画因此整个消失（现象：点下去
      没有滑动，状态已经是新的）。

      但详情确实要跟着开关刷新：停用会把目录搬进 BepInEx/cache/modmanager_disabled_maps/，详情里的「路径」
      跟着变；而详情挂在 `n-collapse-transition` 的稳定插槽里，父级换了数据它
      也不重新求值。这份刷新交给 `detail-key`：外壳与开关保持同一个 DOM 节点，
      只让详情子树按状态重建。
    -->
    <LocalItemCard v-for="set in sets" :key="set.dir_name"
      :detail-key="`${set.dir_name}:${set.enabled}`"
      :data-level-dir="set.dir_name"
      :data-focus-id="ctx.focusIdOf(set)"
      :enabled="set.enabled"
      :toggle-loading="!!ctx.busy[set.dir_name]"
      :title="levelSetDisplayName(set, locale)"
      :title-tooltip="set.error ? translateBackendMessage(set.error) : levelSetDisplayName(set, locale)"
      :version="formatVersion(set.version) || undefined"
      :disabled="!set.enabled"
      :expanded="ctx.expansion.isOuterOpen(set.dir_name)"
      :focus-flash="!!ctx.highlightId && ctx.highlightId === ctx.focusIdOf(set)"
      :tools-min-width="ctx.toolsMinWidth"
      :tool-gap="TOOL_GAP"
      :tool-button-width="TOOL_BUTTON_WIDTH"
      @toggle="(value: boolean) => ctx.toggle(set, value)"
      @activate="(card: HTMLElement) => ctx.activate(set, card)">
      <!--
        状态区：快捷动作 + 标签组。
        与 MOD 管理页同一规则（`levelSetCardAction`）：依赖版本冲突 →「处理
        冲突」跳 MOD 页；地图可更新 →「更新」就地走安装前检查 + 下载导入；
        缺依赖且线上有能补上的仓库卡片 →「去安装」跳那张卡。没有可执行目标
        就不出按钮 —— 一个点了没反应的按钮比没有更糟。
      -->
      <template #status>
        <template v-for="action in actionsOf(set)" :key="action.kind">
          <n-button size="tiny" :style="buttonColorFor(action.tone, 'soft')"
            :title="t(action.label)" :loading="ctx.actionBusy(set)"
            :disabled="ctx.actionDisabled(set)" @click.stop="ctx.runCardAction(set)">
            <component :is="ACTION_ICONS[action.kind]" width="14" height="14" />
            {{ t(action.label) }}
          </n-button>
          <span class="tool-divider" />
        </template>
        <span class="tag-group">
          <n-tag v-for="tag in ctx.tagsOf(set)" :key="tag.kind + tag.text" size="tiny"
            :bordered="false" class="state-tag"
            :class="{ 'state-tag--link': !!(tag.repositoryId || tag.mapId) }"
            :color="tagColorForAny(tag.tone)" :title="ctx.tagTitle(set, tag)"
            @click.stop="ctx.onTagClick(tag)">
            {{ t(tag.text) }}
          </n-tag>
        </span>
      </template>

      <!-- 工具栏：打开目录、删除（游戏运行时禁止删除） -->
      <template #tools>
        <n-button size="tiny" quaternary :title="t('modsLocal.revealInFolder')"
          @click.stop="ctx.openDir(set)">
          <FolderOpenOutline width="14" height="14" />
        </n-button>
        <n-button size="tiny" quaternary type="error" :title="t('modsLocal.delete')"
          :disabled="ctx.kitchenRunning || !!ctx.deleting[set.dir_name]"
          :loading="!!ctx.deleting[set.dir_name]" @click.stop="ctx.removeSet(set)">
          <TrashOutline width="14" height="14" />
        </n-button>
      </template>

      <!-- 展开：元数据 + 绑定地图 + 框架依赖 -->
      <template #detail>
        <div class="detail-grid">
          <!--
            固定四行版式（爸爸定的）：中文名+作者中文名 / 英文名+作者英文名 /
            版本+关卡数 / 路径。两列网格按 DOM 顺序两两成行，所以这里必须两个
            一对地交错写，不能按「先列四个名字」的语义顺序排。
            info 里本来就存了中/英两个集名，作者的译名则来自绑定地图源的
            `group_i18n`；缺哪个都显示 `—`，而不是留空让行消失。
          -->
          <div class="detail-row">
            <span class="detail-label">{{ t('levelSet.nameZh') }}</span>
            <span class="detail-value">{{ set.name_zh || '—' }}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('levelSet.authorZh') }}</span>
            <span class="detail-value">{{ ctx.authorNamesOf(set).zh || '—' }}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('levelSet.nameEn') }}</span>
            <span class="detail-value">{{ set.name || '—' }}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('levelSet.authorEn') }}</span>
            <span class="detail-value">{{ ctx.authorNamesOf(set).en || '—' }}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('levelSet.version') }}</span>
            <span class="detail-value">{{ set.version || '—' }}</span>
          </div>
          <div class="detail-row">
            <span class="detail-label">{{ t('levelSet.levelCountLabel') }}</span>
            <span class="detail-value">{{ set.level_count }}</span>
          </div>
          <!-- 绝对路径占满整行：半列宽度只够显示到盘符 -->
          <div class="detail-row detail-row--wide">
            <span class="detail-label">{{ t('levelSet.path') }}</span>
            <span class="detail-value mono detail-value--path"
              :title="set.path || undefined">{{ set.path || '—' }}</span>
          </div>
        </div>

        <!--
          线上绑定与状态单独一节：这几项说的是「这张本地图在线上/游戏侧是
          什么情况」，与上面四行「这张图本身是什么」不是同一类信息，混着排
          会读不清，也会把上面固定的四行版式撑乱。
        -->
        <div class="detail-section">
          <div class="detail-section-title">{{ t('levelSet.bindStatusSection') }}</div>
          <div class="detail-grid">
            <div class="detail-row detail-row--wide">
              <span class="detail-label">{{ t('levelSet.boundMap') }}</span>
              <span class="detail-value">
                <template v-if="ctx.bindingOf(set)">
                  {{ ctx.bindingOf(set)?.name }}
                  <span v-if="ctx.bindingOf(set)?.tag" class="mono bound-tag">{{ ctx.bindingOf(set)?.tag }}</span>
                  <!-- 反方向跳转：从管理页看这张图在下载页的样子 -->
                  <n-button size="tiny" text type="primary" class="bound-link"
                    @click.stop="ctx.linkToMap(ctx.bindingOf(set)?.map.id)">
                    {{ t('levelSet.viewInMapDownload') }}
                  </n-button>
                </template>
                <template v-else>—</template>
              </span>
            </div>
            <!-- 解析失败：原因放在详情里，卡片行只留标签 -->
            <div v-if="set.error" class="detail-row detail-row--wide">
              <span class="detail-label">{{ t('levelSet.parseError') }}</span>
              <span class="detail-value is-error">{{ translateBackendMessage(set.error) }}</span>
            </div>
            <div v-if="ctx.hintOf(set)" class="detail-row detail-row--wide">
              <span class="detail-label">{{ t('levelSet.statusHint') }}</span>
              <span class="detail-value">{{ ctx.hintOf(set) }}</span>
            </div>
          </div>
        </div>

        <!--
          随包插件：与地图同处一个父目录、跟着整个集目录一起启停的 DLL。
          它们是这张图的一部分（不是独立 MOD），所以复用 MOD 卡片的下沉
          版本挂在这里。**没有开关**：启停由集目录的 rename 决定，单个
          DLL 没有属于自己的开关状态，给一个会和集目录打架。点击卡片
          展开这个 DLL 的元数据；安装侧读 ZIP 条目，这里读磁盘目录，
          同一份 `mods::read_dll_details` 解析，只是没有 ZIP 内路径。
          「这是随包插件」与版本号都跟在标题后面（`version` + `titleTag`），
          不做左侧缩进：缩进只能表达从属，代价是整行往右推、右侧那列被挤窄。
          这一节**不写分节标题**：卡片右端已经有「随包插件」标签，再加一行
          「随包插件（N）」是把同一句话说两遍，还平白占掉一行高度。
        -->
        <div v-if="set.plugins?.length" class="detail-section">
          <div class="plugin-list">
            <!--
              开关由集目录整体 rename 控制，这张卡没有开关；开关键与值照传 ——
              卡片判定开关值只有「快照优先、回退 props」一条规则，不按是否
              可开关分叉，页面插槽也因此统一在这里读一次快照。
            -->
            <LocalItemCard v-for="plugin in set.plugins" :key="plugin.file_name"
              :data-plugin-of="set.dir_name"
              :data-plugin-file="plugin.file_name"
              :switchable="false"
              :enabled="plugin.enabled"
              :title="ctx.pluginTitle(plugin)"
              :title-tooltip="plugin.file_name"
              :version="formatVersion(plugin.version) || undefined"
              :title-tag="t('levelSet.bundledPlugin')"
              :disabled="!plugin.enabled"
              :expanded="ctx.expansion.isInnerOpen(set.dir_name, plugin.file_name)"
              @activate="(card: HTMLElement) => ctx.togglePlugin(set, plugin, card)">
              <template #detail>
                <div class="detail-grid">
                  <div class="detail-row">
                    <span class="detail-label">{{ t('modsLocal.name') }}</span>
                    <span class="detail-value">{{ plugin.name || '—' }}</span>
                  </div>
                  <div class="detail-row">
                    <span class="detail-label">GUID</span>
                    <span class="detail-value mono">{{ plugin.guid || '—' }}</span>
                  </div>
                  <div class="detail-row">
                    <span class="detail-label">{{ t('modsOnline.versions') }}</span>
                    <span class="detail-value mono">{{ formatVersion(plugin.version) || '—' }}</span>
                  </div>
                  <div class="detail-row">
                    <span class="detail-label">{{ t('modsLocal.title') }}</span>
                    <span class="detail-value">{{ plugin.title || '—' }}</span>
                  </div>
                  <div class="detail-row">
                    <span class="detail-label">{{ t('modsLocal.productName') }}</span>
                    <span class="detail-value">{{ plugin.product_name || '—' }}</span>
                  </div>
                  <div class="detail-row">
                    <span class="detail-label">{{ t('modsLocal.author') }}</span>
                    <span class="detail-value">{{ plugin.company || '—' }}</span>
                  </div>
                  <div class="detail-row">
                    <span class="detail-label">{{ t('modsLocal.originalFilename') }}</span>
                    <span class="detail-value mono">{{ plugin.original_filename || '—' }}</span>
                  </div>
                  <div class="detail-row">
                    <span class="detail-label">{{ t('modsLocal.role') }}</span>
                    <span class="detail-value">
                      {{ plugin.role === 'dependency'
                        ? t('tools.dependency')
                        : t('levelSet.pluginRoleMod') }}
                    </span>
                  </div>
                  <div class="detail-row">
                    <span class="detail-label">{{ t('modsLocal.enabled') }}</span>
                    <span class="detail-value">
                      {{ plugin.enabled ? t('modsLocal.enabled2') : t('modsLocal.disabled') }}
                    </span>
                  </div>
                  <div class="detail-row">
                    <span class="detail-label">{{ t('modsLocal.file2') }}</span>
                    <span class="detail-value mono">{{ plugin.file_name }}</span>
                  </div>
                </div>
              </template>
            </LocalItemCard>
          </div>
        </div>

        <!--
          依赖声明：本地缺失的标红，用户重新启用前就能看到；与下载页详情、
          MOD 安装页详情共用同一张依赖表（名称是人类可读的仓库名/包名，
          不再需要等宽字体）。没有声明时整块不渲染，不留空分隔线。
        -->
        <div v-if="ctx.dependencyRows(set).length" class="detail-section detail-section--table">
          <DependencyTable :rows="ctx.dependencyRows(set)" @jump="ctx.openDependencyJump" />
        </div>
      </template>
    </LocalItemCard>
  </div>
</template>

<style scoped>
.map-grid {
  /* 每个卡片撑满容器宽度（单列纵向堆叠） */
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 2px 0 6px 14px;
}

.tag-group {
  display: flex;
  flex-wrap: nowrap;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
  min-width: 0;
  overflow: hidden;
}

/* 可点标签的手型与悬停反馈由 global.css 的 .state-tag--link 统一提供 */

/* 详情：两列信息网格，值允许换行（作者名 / 状态说明可能很长） */
.detail-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 6px 18px;
}

.detail-row {
  display: flex;
  gap: 10px;
  min-width: 0;
  font-size: 12px;
  line-height: 1.5;
}

/* 绝对路径单独占一行：半列宽度只够显示到盘符 */
.detail-row--wide {
  grid-column: 1 / -1;
}

.detail-label {
  flex: 0 0 auto;
  min-width: 64px;
  color: var(--oc2-text-3);
}

.detail-value {
  min-width: 0;
  color: var(--oc2-text-2);
  overflow-wrap: anywhere;
}

.detail-value.is-error {
  color: var(--oc2-tone-critical-text);
}

/*
 * 路径单行不折行：折成两三行的绝对路径比省略号更难读，全文放在 title 里。
 * 需要 `overflow: hidden` 才轮得到 `text-overflow`，否则只是溢出被裁。
 */
.detail-value--path {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 等宽文本用全局 `.mono`（styles/global.css），这里只补详情行的间距 */
.bound-tag {
  margin-left: 6px;
  color: var(--oc2-text-3);
}

.bound-link {
  margin-left: 8px;
}

.detail-section {
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px dashed var(--oc2-row-line);
}

/*
 * 依赖表专用的那一档：表格自己就有一圈边框和圆角，再叠一条虚线加两段留白，
 * 等于把「这里换内容了」说两遍。这里只保留卡片内统一的 8px 节奏，不再画分隔线。
 * 与 MOD 管理页（ModsLocalView.vue）同名同值 —— 两个页面的详情区都是块流
 * （没有 flex gap 兜底），所以两处都是 8px。
 */
.detail-section--table {
  margin-top: 8px;
  padding-top: 0;
  border-top: 0;
}

/*
 * 随包插件列表：与 MOD 管理页「包内子卡片」同一套视觉 —— 下沉底色，
 * 让人看出它们是这张图的附属物而不是同级条目。
 * 不再左缩进：「从属」由卡片标题后的「随包插件」标签说明，缩进只会把标题和
 * 版本号一起往右推，右侧留白反而更多。
 * `--card-bg` 是 `LocalItemCard` 自己读的变量，在容器上下发就能让每张子卡
 * 换成下沉底，不必去改组件内部样式。
 */
.plugin-list {
  display: grid;
  gap: 8px;
  --card-bg: var(--oc2-sunken);
}

/* 分节小标题：只做分组提示，不抢上面四行主体的视觉重量 */
.detail-section-title {
  margin-bottom: 6px;
  font-size: 12px;
  color: var(--oc2-text-3);
}
</style>
