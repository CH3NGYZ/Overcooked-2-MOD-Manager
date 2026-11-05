<script setup lang="ts">
import { computed, inject } from 'vue'
import { NButton, NCollapseTransition, NSwitch, NTag } from 'naive-ui'
import {
  ArrowUpCircleOutline,
  DocumentTextOutline,
  DownloadOutline,
  FolderOpenOutline,
  RefreshOutline,
  SettingsOutline,
  TrashOutline,
} from '@vicons/ionicons5'
import { t } from '../i18n'
import { formatVersion } from '../modStatus'
import { buttonColorFor, tagColorFor, tagColorForAny } from '../theme'
import DependencyTable from './DependencyTable.vue'
import LocalItemCard from './LocalItemCard.vue'
import ModDetailGrid from './ModDetailGrid.vue'
import ModStatusTags from './ModStatusTags.vue'
import { MOD_GROUP_CTX, TOOL_BUTTON_WIDTH, TOOL_GAP } from './levelSetGroupContext'

/**
 * 一个分类里的 MOD 卡片与多 DLL 包外壳（`ModsLocalView` 折叠项的内容区）。
 *
 * 为什么不写在页面的插槽里：与地图管理页同一个原因 —— `n-collapse` 只有
 * `expanded-names` 一个动态 prop，同组增删卡时组集合没变、数组引用没变，
 * Vue 会整棵跳过子树，插槽闭包不重跑、组内 `v-for` 停在旧数据上。
 * 组件实例有独立渲染 effect，上下文一变就自己重渲。
 */
const props = defineProps<{ groupKey: string }>()

const ctx = inject(MOD_GROUP_CTX)!

/** 本分类的包：键是永不变化的分类键，数据变了这里自己重算。 */
const packages = computed(
  () => ctx.groups.find((group) => group.key === props.groupKey)?.packages ?? [],
)
</script>

<template>
  <div class="mod-grid">
    <template v-for="pkg in packages" :key="pkg.key">
      <!--
        单 DLL 包：不套外壳，直接就是那张卡片。
        外壳（开关 + 标题 + 标签区 + 工具栏 + 展开）由 LocalItemCard 负责，
        这里只提供数据、状态插槽与动作；原生 click 仍由页面处理 ——
        onCardClick 要给 document 级的「点空白收起」打标记。

        `switchable` 只对随包依赖关掉：没有 BepInPlugin GUID 的库 DLL
        不是独立 MOD，启停跟着所属包走，单独关掉只会让依赖它的主 MOD
        加载失败（随包插件同理，见地图管理页的详情）。
      -->
      <LocalItemCard v-if="!pkg.grouped" v-for="mod in pkg.mods" :key="mod.dll_file"
        :data-expand-key="ctx.modCardKey(mod.dll_file)"
        :data-dll-file="mod.dll_file"
        :data-focus-id="ctx.bindingRepositoryId(mod)"
        :data-mod-anchor="ctx.modAnchor(mod)"
        :switchable="!mod.is_dependency"
        :enabled="mod.enabled"
        :toggle-loading="!!ctx.busy[mod.dll_file]"
        :title="ctx.singleModTitle(mod)"
        :version="formatVersion(mod.version) || undefined"
        :title-tag="mod.is_dependency ? t('modsLocal.bundledDependency') : undefined"
        :disabled="!mod.enabled"
        :expanded="ctx.expansion.isOuterOpen(ctx.modCardKey(mod.dll_file))"
        :focus-flash="ctx.highlightId === ctx.bindingRepositoryId(mod)"
        :tools-min-width="ctx.toolsMinWidth"
        :tool-gap="TOOL_GAP"
        :tool-button-width="TOOL_BUTTON_WIDTH"
        @toggle="(value: boolean) => ctx.toggle(mod, value)"
        @click="ctx.onCardClick($event, mod)">
        <template #status>
          <!--
            状态快捷动作：按钮由当前最需要注意的状态决定
            （可更新 → 更新、仓库刷新失败 → 刷新），
            规则集中在 statusActions.ts。与标签之间用竖线隔开，
            视觉上属于标签区的一部分（不是右侧工具栏的常规动作）。
          -->
          <template v-if="ctx.actionByDll[mod.dll_file]">
            <!-- 配色跟随触发它的状态档位，来自 theme.ts 的统一色阶 -->
            <n-button size="tiny" :style="buttonColorFor(ctx.actionByDll[mod.dll_file].tone, 'soft')"
              :title="t(ctx.actionByDll[mod.dll_file].label)"
              :loading="!!ctx.previewing[ctx.bindingRepositoryId(mod) ?? ''] || !!ctx.refreshing[ctx.bindingRepositoryId(mod) ?? '']"
              :disabled="ctx.kitchenRunning" @click.stop="ctx.runCardAction(mod)">
              <ArrowUpCircleOutline v-if="ctx.actionByDll[mod.dll_file].kind === 'update'" width="14" height="14" />
              <DownloadOutline v-else-if="ctx.actionByDll[mod.dll_file].kind === 'install-dependency'" width="14" height="14" />
              <TrashOutline v-else-if="ctx.actionByDll[mod.dll_file].kind === 'uninstall-conflict'" width="14" height="14" />
              <RefreshOutline v-else width="14" height="14" />
              {{ t(ctx.actionByDll[mod.dll_file].label) }}
            </n-button>
            <span class="tool-divider" />
          </template>

          <!--
            状态标签组：宽度按内容自适应，整体贴住分割线左侧。
            status-pulse：刷新完成后标签变化时播一次短暂高亮（见 isStatusPulsing）。
          -->
          <span class="tag-group" :class="{ 'status-pulse': ctx.isStatusPulsing(mod) }">
            <!--
              配色走 Naive n-tag 原生 :color，由 theme.ts 统一计算。
              「缺依赖」/「冲突」跳到**依赖的**仓库；解析不出可安装来源时
              没有跳转目标，此时不给链接样式也不可点 —— 一个点了没反应、
              却长得像能点的标签比纯文字更糟。
            -->
            <ModStatusTags :dll-file="mod.dll_file" />
          </span>
        </template>

        <!-- 工具栏：可选按钮在最左（用占位保持列宽），固定按钮贴右 -->
        <template #tools>
          <!-- 查看日志：只有 BepInEx/logs/<guid> 存在时才显示；缺失时保留同宽占位 -->
          <n-button v-if="mod.has_logs" size="tiny" quaternary :title="t('modsLocal.viewLogs')"
            class="tool-optional" @click.stop="ctx.openLogs(mod)">
            <DocumentTextOutline width="14" height="14" />
          </n-button>
          <span v-else class="tool-placeholder" />
          <!-- 配置编辑：只有存在配置文件的 MOD 才有；缺失时保留同宽占位，
               这样「打开位置」「删除」始终落在最右侧同一列 -->
          <n-button v-if="mod.has_config" size="tiny" quaternary :title="t('tools.editConfig')"
            class="tool-optional" @click.stop="ctx.editConfig(mod)">
            <SettingsOutline width="14" height="14" />
          </n-button>
          <span v-else class="tool-placeholder" />
          <n-button size="tiny" quaternary :title="t('modsLocal.revealInFolder')"
            :disabled="ctx.kitchenRunning" @click.stop="ctx.openLocation(mod)">
            <FolderOpenOutline width="14" height="14" />
          </n-button>
          <n-button size="tiny" quaternary type="error" :disabled="ctx.kitchenRunning" :title="t('modsLocal.delete')"
            @click.stop="ctx.remove(mod)">
            <TrashOutline width="14" height="14" />
          </n-button>
        </template>

        <!-- 不阻止冒泡：点详情里的任意位置都算「点空白」，由 onDocumentClick 收起 -->
        <template #detail>
          <ModDetailGrid :mod="mod" :binding-text="ctx.bindingText(mod)"
            :status-summary="ctx.statusSummary(mod)" :package-count-text="ctx.packageCountText(mod)" />
          <!--
            依赖声明：和地图管理页详情一样，表格直接贴着上面的安装信息
            （`.detail-section--table` 去掉虚线与额外留白，只留 8px 节奏），
            因为表格自己就有一圈边框和圆角。没有声明时整块都不渲染。
          -->
          <div v-if="ctx.modDependencyRows(mod).length" class="detail-section detail-section--table">
            <DependencyTable :rows="ctx.modDependencyRows(mod)" @jump="ctx.openDependencyJump" />
          </div>
        </template>
      </LocalItemCard>

      <!--
        多 DLL 包：折叠外壳 + 包级操作，内部仍是同样的卡片。
        跨页跳转（从 MOD 安装页点标签过来）要落在**包外壳**上：
        包是页面上的一个展示单元，跳转进来高亮整包才符合语义；
        包内子 DLL 不是单元，所以它们不带 data-focus-id。
      -->
      <section v-else class="package-shell"
        :data-expand-key="pkg.key"
        :data-focus-id="pkg.repositoryId"
        :data-mod-anchor="`unit:${pkg.key}`"
        :class="{
          expanded: ctx.expansion.isOuterOpen(pkg.key),
          'focus-flash': !!pkg.repositoryId && ctx.highlightId === pkg.repositoryId,
        }">
        <header class="package-head"
          @click="ctx.togglePackage(pkg, $event)">
          <!-- 左：开关 + 包名 + 子 MOD 数量（数量紧跟包名；「N 个」已表明这是包） -->
          <div class="package-left">
            <!--
              包级开关三态：全开 / 全关 / 部分。
              Naive 的 indeterminate 会画一个加载圈，不像「开关中间态」，
              所以这里仍用布尔值，靠 is-partial 类把滑块和轨道做半透明表达。
              title 说明它会一起切换包内每个子 MOD —— 只写「N 个」时
              用户未必意识到这一个开关等于四个。
            -->
            <n-switch class="package-switch" size="small" :value="pkg.enabled"
              :loading="pkg.mods.some((m) => ctx.busy[m.dll_file])"
              :title="t('modsLocal.packageSwitchHint', { count: pkg.mods.length })"
              :class="{ 'is-partial': pkg.partial }"
              @click.stop @update:value="(value: boolean) => ctx.togglePackageEnabled(pkg, value)" />
            <b class="package-title" :title="pkg.label">{{ pkg.label }}</b>
            <n-tag size="tiny" :bordered="false" class="state-tag" :color="tagColorFor('muted')">
              {{ t('rename.text', { count: pkg.mods.length }) }}
            </n-tag>
          </div>

          <!-- 右：状态标签 + 分割线 + 工具列（与单 MOD 卡片同一套列宽预留） -->
          <div class="package-right">
            <!--
              包级快捷动作：与单 MOD 卡片共用 statusActions 的规则。
              包是一个整体，动作针对的是这个包绑定的仓库，
              不是包内某个 DLL。
            -->
            <template v-if="pkg.action">
              <n-button size="tiny" :style="buttonColorFor(pkg.action.tone, 'soft')"
                :title="t(pkg.action.label)"
                :loading="!!ctx.previewing[pkg.repositoryId ?? ''] || !!ctx.refreshing[pkg.repositoryId ?? '']"
                :disabled="ctx.kitchenRunning"
                @click.stop="ctx.runPackageAction(pkg)">
                <ArrowUpCircleOutline v-if="pkg.action.kind === 'update'" width="14" height="14" />
                <DownloadOutline v-else-if="pkg.action.kind === 'install-dependency'" width="14" height="14" />
                <TrashOutline v-else-if="pkg.action.kind === 'uninstall-conflict'" width="14" height="14" />
                <RefreshOutline v-else width="14" height="14" />
                {{ t(pkg.action.label) }}
              </n-button>
              <span class="package-divider" />
            </template>

            <!--
              包状态标签：以包为主体。版本结论（最新版 / 可更新 / 刷新中…）
              整包只有一条、不带数量；成员级告警（重复 / 低版本 / 加载警告）
              才带数量，因为确实可能只命中包内某几个 DLL。
              配色与单 MOD 行同源，点击跳到 MOD 安装页对应仓库。
            -->
            <span class="tag-group">
              <n-tag v-for="entry in pkg.statuses" :key="entry.key" size="tiny" :bordered="false"
                class="state-tag" :class="{ 'state-tag--link': !!ctx.packageTagTargetId(pkg, entry.key) }"
                :color="tagColorForAny(entry.tone)"
                :title="ctx.packageTagTitle(pkg, entry.key)"
                @click.stop="ctx.openPackageTag(pkg, entry.key)">
                {{ entry.count ? `${entry.count} ${t(entry.text)}` : t(entry.text) }}
              </n-tag>
              <n-tag v-if="!pkg.enabled && !pkg.partial" size="tiny" :bordered="false"
                class="state-tag" :color="tagColorFor('muted')">{{ t('modsLocal.disabled') }}</n-tag>
              <n-tag v-else-if="pkg.partial" size="tiny" :bordered="false"
                class="state-tag" :color="tagColorFor('accent')">{{ t('tools.partiallyEnabled') }}</n-tag>
            </span>

            <span class="package-divider" />

            <!--
              包标题与单 MOD 卡片是同一层级，工具栏必须用同一列宽，
              这样删除按钮落在同一竖线；只有一个按钮时其余位置留空。
            -->
            <span class="package-tool-group" :style="ctx.toolGroupStyle" @click.stop>
              <n-button size="tiny" quaternary type="error" :title="t('modsLocal.deletePackage')"
                :disabled="ctx.kitchenRunning" @click="ctx.removePackage(pkg)">
                <TrashOutline width="14" height="14" />
              </n-button>
            </span>
          </div>
        </header>

        <!-- 包展开/收起：同样用 Naive 的折叠过渡 -->
        <n-collapse-transition :show="ctx.expansion.isOuterOpen(pkg.key)" display-directive="show">
        <div class="package-body">
          <!--
            包内子 DLL 行不是独立单元：不加定位锚点，也不参与状态高亮。
            状态结论属于整个包，高亮只画在外壳上（见 package-shell）。
            外壳仍由 LocalItemCard 提供；这些行没有状态标签，
            所以不传 #status，组件会自己补 1px 隐形占位保持按钮列对齐。
            顺序是「主 MOD → 它带的依赖 DLL」：标题后带版本号，依赖项多一个
            灰色「随包依赖」标签（排序见 toPackage）。
            依赖行同样不给开关（理由见上面单 DLL 包那段注释）。
          -->
          <LocalItemCard v-for="mod in pkg.mods" :key="mod.dll_file"
            :data-dll-file="mod.dll_file"
            :switchable="!mod.is_dependency"
            :enabled="mod.enabled"
            :toggle-loading="!!ctx.busy[mod.dll_file]"
            :title="ctx.modTitle(mod)"
            :version="formatVersion(mod.version) || undefined"
            :title-tag="mod.is_dependency ? t('modsLocal.bundledDependency') : undefined"
            :disabled="!mod.enabled"
            :expanded="ctx.expansion.isInnerOpen(pkg.key, mod.dll_file)"
            :tools-min-width="ctx.toolsMinWidth"
            :tool-gap="TOOL_GAP"
            :tool-button-width="TOOL_BUTTON_WIDTH"
            @toggle="(value: boolean) => ctx.toggle(mod, value)"
            @click="ctx.onCardClick($event, mod, pkg.key)">
            <!--
              包内子 MOD 只保留三个操作（标签统一在包标题行）。
            -->
            <template #tools>
              <n-button v-if="mod.has_logs" size="tiny" quaternary :title="t('modsLocal.viewLogs')"
                class="tool-optional" @click.stop="ctx.openLogs(mod)">
                <DocumentTextOutline width="14" height="14" />
              </n-button>
              <span v-else class="tool-placeholder" />
              <n-button v-if="mod.has_config" size="tiny" quaternary :title="t('tools.editConfig')"
                class="tool-optional" @click.stop="ctx.editConfig(mod)">
                <SettingsOutline width="14" height="14" />
              </n-button>
              <span v-else class="tool-placeholder" />
              <n-button size="tiny" quaternary :title="t('modsLocal.revealInFolder')"
                :disabled="ctx.kitchenRunning" @click.stop="ctx.openLocation(mod)">
                <FolderOpenOutline width="14" height="14" />
              </n-button>
              <n-button size="tiny" quaternary type="error" :disabled="ctx.kitchenRunning"
                :title="t('modsLocal.delete')" @click.stop="ctx.remove(mod)">
                <TrashOutline width="14" height="14" />
              </n-button>
            </template>

            <!-- 同上：详情区不拦冒泡，点任意位置都能收起 -->
            <template #detail>
              <ModDetailGrid :mod="mod" :binding-text="ctx.bindingText(mod)"
                :status-summary="ctx.statusSummary(mod)" :package-count-text="ctx.packageCountText(mod)" />
            </template>
          </LocalItemCard>
          <!--
            整包共用一份依赖声明（关系挂在仓库上），所以只出一张表。
            放整批子卡片**之后**：先看包里有什么（本体与它带的依赖），
            再看这个包对外要求什么；和地图管理页「信息 → 随包插件 →
            依赖声明」同一顺序，两页不用记两套版式。
          -->
          <div v-if="ctx.packageDependencyRows(pkg).length" class="detail-section detail-section--table">
            <DependencyTable :rows="ctx.packageDependencyRows(pkg)" @jump="ctx.openDependencyJump" />
          </div>
        </div>
        </n-collapse-transition>
      </section>
    </template>
  </div>
</template>

<style scoped>
.mod-grid {
  /* 每个卡片撑满容器宽度（单列纵向堆叠） */
  display: flex;
  flex-direction: column;
  gap: 10px;
  margin: 2px 0 6px 14px;
}

/*
 * MOD 包外壳（2 个及以上 DLL 才有）。
 * 视觉上与 DLL 卡片保持同一套语言：同样的圆角、同样的边框与底色，
 * 只是内部多一层缩进和分隔线，让人看出「这是一个包含多个 DLL 的包」。
 */
.package-shell {
  display: flex;
  flex-direction: column;
  border: 1px solid var(--oc2-row-line);
  border-radius: 12px;
  background: var(--oc2-card);
  transition: border-color .16s ease, background-color .16s ease;
}
.package-shell:hover { border-color: color-mix(in srgb, var(--oc2-accent) 55%, var(--oc2-row-line)); }
/*
 * 包外壳展开：与单 MOD 卡片展开同一套视觉（accent 边框 + 8% accent 底色）。
 * 不给外壳规则时，收起和展开长得一模一样，用户看不出「这个包点开了」。
 * 底色按 `--oc2-card` 混合（不是 `--card-bg`）：包内子卡片用下沉底色
 * （`.package-body .mod-card { --card-bg: var(--oc2-sunken) }`），外壳若跟着
 * 子卡的基色走，展开后两者同色，包的边界就消失了。
 * 放在 :hover 之后与 `.mod-card.expanded` 顺序一致；后面的 .focus-flash
 * 用 12% 底色，跨页定位高亮仍会盖过展开态。
 */
.package-shell.expanded {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 8%, var(--oc2-card));
}
.package-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 14px;
  /*
   * 标题行本身就在外壳边框之内，14px 内边距与 .mod-card 一致即可对齐；
   * 包内卡片额外用 margin-right: -1px 抵消外壳边框，两边内容右缘因此相等。
   */
  padding: 10px 14px;
  cursor: pointer;
  user-select: none;
  min-width: 0;
}
/*
 * 包标题行**不吸顶**，与包内子卡片（`LocalItemCard`）一致：展开时由
 * `expandScroll.ts` 的一次性滚动把外壳夹在页头下沿，滚动过程中标题跟着内容走。
 */
/* 左半：开关 + 包名 + 子 MOD 数量（数量紧跟包名） */
.package-left {
  display: flex;
  align-items: center;
  gap: 11px;
  flex: 1;
  min-width: 0;
}
/*
 * 右半：状态标签 + 分割线 + 工具列。
 * 结构与单 MOD 卡片的 .card-right 完全一致，因此标签列与按钮列
 * 会和下方所有卡片对齐在同一竖直线上。
 */
.package-right {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  flex-wrap: nowrap;
  gap: 10px;
  flex: 0 0 auto;
  min-width: 0;
}
/*
 * 包标题按文字宽度显示（不撑满），后面紧跟「N 个」标签，
 * 剩余空间交给左侧容器的 flex 空隙，避免长标题把右侧标签和工具栏推出去。
 */
.package-title {
  flex: 0 1 auto;
  min-width: 0;
  max-width: 60%;
  font-size: 13.5px;
  font-weight: 600;
  color: var(--oc2-text-1);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 包级开关：部分启用时轨道半透明，滑块停在中间位置 */
.package-switch.is-partial :deep(.n-switch__rail) {
  background-color: color-mix(in srgb, var(--oc2-accent) 45%, transparent);
}
.package-switch.is-partial :deep(.n-switch__button) {
  /* 滑块水平居中，表达「既非全开也非全关」 */
  transform: translateX(calc(50% - 1px));
}
/*
 * 包标题行的开关 / 分割线 / 工具列。
 *
 * 这几套样式必须留在父页面：卡片里的同名元素由 LocalItemCard 渲染
 * （在组件自己的 scoped 样式里），包标题行则由父页面直接渲染。
 * scoped 是逐元素生效的，两边各留一份，不能指望一条规则同时打两边。
 */
.package-switch { flex: 0 0 auto; }
.package-divider {
  width: 1px;
  height: 16px;
  flex: 0 0 1px;
  background: var(--oc2-row-line);
}
/*
 * 包工具栏：与卡片内 .tool-group 用同一套列宽。
 * gap 与按钮宽度来自 toolGroupStyle 下发的 CSS 变量（与 toolColumnWidth 同源），
 * 所以包标题行与所有卡片的按钮列落在同一条竖线上。
 */
.package-tool-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: var(--tool-gap, 8px);
  flex: 0 0 auto;
}
.package-tool-group :deep(.n-button) {
  width: var(--tool-button-width, 30px);
  min-width: var(--tool-button-width, 30px);
  padding: 0;
}
/*
 * 包内 DLL 列表：外壳留 14px 内边距 + 1px 边框。
 *
 * 子 MOD 行不必和单 MOD 行左对齐——它嵌在外壳里，左侧天然多一层外壳
 * 边框和内边距。约束只在右侧：让「按钮右边缘 → 子卡片边框」与
 * 「子卡片边框 → 外壳边框」两段相等，看起来才是同一套留白。
 */
.package-body {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 14px;
  border-top: 1px solid var(--oc2-row-line);
}
/*
 * 包内卡片：把卡片自身 1px 右边框也算进「卡片外沿」，让两段留白相等。
 *
 * 目标：按钮右边缘 → 子卡片右边框外沿 = 子卡片右边框外沿 → 外壳边框内沿。
 * 外壳内容区已留 14px，所以子卡片只需再留 `14px - 1px(自身边框) = 13px`，
 * 两段量出来才是 14 / 14 而不是 15 / 14。
 *
 * 底色只改变量不给背景：卡片内部还有 :hover / .expanded 会换底色，
 * 在这里再写 background 就成了两个同优先级的规则抢同一属性，
 * 结果取决于样式表顺序。--card-bg 是卡片自己的默认底色来源。
 */
.package-body .mod-card {
  --card-bg: var(--oc2-sunken);
  padding-right: calc(14px - 1px);
}

/*
 * 详情区的依赖表分隔线：与地图管理页（LevelsView.vue）里那条 `.detail-section`
 * **同名同值**。scoped 样式逐元素生效，两页各自编译一份、互相看不见，所以这不是
 * 重复定义而是必须的两份；改这里必须同时改那边，否则同一张依赖表在两个页面的
 * 上下留白会不一样。
 */
.detail-section {
  margin-top: 10px;
  padding-top: 8px;
  border-top: 1px dashed var(--oc2-row-line);
}

/*
 * 依赖表专用的那一档：表格自己就有一圈边框和圆角，再叠一条虚线加两段留白，
 * 等于把「这里换内容了」说两遍；包内还会把「子卡片 → 依赖表」推得比子卡片
 * 之间（8px）更松。这里只保留卡片内统一的 8px 节奏，不再画分隔线。
 *
 * 包内 `.package-body` 是 flex + `gap: 8px`，间距已经由 gap 给过，再叠 margin
 * 就是 16px，所以那一处归零。与地图管理页（LevelsView.vue）同名同值。
 */
.detail-section--table {
  margin-top: 8px;
  padding-top: 0;
  border-top: 0;
}
.package-body .detail-section--table { margin-top: 0; }

/*
 * 卡片自身的样式（.mod-card / .card-row / .card-* / .tool-*）已随外壳一起
 * 下沉到 LocalItemCard.vue 的 scoped 样式里。卡片是子组件渲染的 DOM，
 * 父级 scoped 规则只对父级自己的元素生效，留在这里只会变成永不命中的死规则。
 * 父页面只保留包外壳（.package-*）和自己编译的插槽内容（.tag-group 等）。
 *
 * 定位高亮脉冲：卡片用组件自己的 local-item-card-focus-pulse，
 * 这里这份 keyframes 只服务下面的 .package-shell（包高亮画在外壳上）。
 */
@keyframes focus-pulse {
  0%   { box-shadow: 0 0 0 0 color-mix(in srgb, var(--oc2-accent) 70%, transparent); }
  40%  { box-shadow: 0 0 0 5px color-mix(in srgb, var(--oc2-accent) 30%, transparent); }
  100% { box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent); }
}
/*
 * 包外壳的跨页定位高亮：与单 MOD 卡片同一套视觉。
 * 包是页面上的一个展示单元，高亮整个外壳，而不是包内某个 DLL 行 ——
 * 结论属于包，不属于某一个子 MOD。
 */
.package-shell.focus-flash {
  border-color: var(--oc2-accent);
  background: color-mix(in srgb, var(--oc2-accent) 12%, var(--oc2-card));
  box-shadow: 0 0 0 2px color-mix(in srgb, var(--oc2-accent) 45%, transparent);
  animation: focus-pulse 1200ms cubic-bezier(0.4, 0, 0.2, 1);
}
@media (prefers-reduced-motion: reduce) {
  .package-shell.focus-flash { animation: none; }
}

/* 可点标签的手型与悬停反馈由 global.css 的 .state-tag--link 统一提供 */

/*
 * 状态变化脉冲：刷新完成、标签从「刷新中」变成确定状态时，
 * 标签组轻微放大后回落并短暂提亮，让异步结果被看见。
 *
 * 用 scale + filter 而不是改颜色：颜色已由 tone 表达语义，
 * 这里只表达「刚刚变了」这一时间信息，两者不冲突。
 * 时长与 PULSE_DURATION（900ms）对齐 —— 计时器在 script 里，动画在 CSS 里，两处必须一致。
 */
.tag-group.status-pulse {
  animation: status-pulse 900ms cubic-bezier(0.4, 0, 0.2, 1);
}
@keyframes status-pulse {
  0%   { transform: scale(1);    filter: brightness(1); }
  22%  { transform: scale(1.06); filter: brightness(1.45); }
  100% { transform: scale(1);    filter: brightness(1); }
}
@media (prefers-reduced-motion: reduce) {
  /* 减少动效：只保留短暂的亮度变化，不做缩放 */
  .tag-group.status-pulse { animation: status-pulse-fade 900ms ease; }
  @keyframes status-pulse-fade {
    0%   { filter: brightness(1); }
    22%  { filter: brightness(1.45); }
    100% { filter: brightness(1); }
  }
}
/*
 * 状态标签组：宽度跟内容走，不再预留固定最小宽度。
 * 之前用 min-width: 128px 让所有行的标签区起点一致，但单个标签时
 * 右侧会空出一大片（截图里高亮框比「最新版」宽一倍）。
 * 现在靠 flex-end + 固定的分割线/工具栏列宽保证右对齐，
 * 标签少时容器就窄，视觉上没有空白。
 *
 * 这条规则留在父页面：包标题行的标签区直接由父模板渲染，
 * 而单 MOD 卡片里的标签组位于 status 插槽内 —— 插槽内容由父页面编译，
 * 带的是父级 scope id，所以父级规则照样命中。
 */
.tag-group {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 6px;
  flex: 0 1 auto;
  flex-wrap: nowrap;
  overflow: hidden;
}
</style>
