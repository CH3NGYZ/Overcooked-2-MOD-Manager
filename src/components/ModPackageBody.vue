<script setup lang="ts">
import { computed, inject } from 'vue'
import { NButton } from 'naive-ui'
import { DocumentTextOutline, FolderOpenOutline, SettingsOutline, TrashOutline } from '@vicons/ionicons5'
import { t } from '../i18n'
import { formatVersion } from '../modStatus'
import DependencyTable from './DependencyTable.vue'
import LocalItemCard from './LocalItemCard.vue'
import ModDetailGrid from './ModDetailGrid.vue'
import { MOD_GROUP_CTX, TOOL_BUTTON_WIDTH, TOOL_GAP } from './levelSetGroupContext'

// 折叠过渡会缓存插槽；包内成员必须在自己的渲染 effect 中读取过滤后的数据。
const props = defineProps<{ groupKey: string; packageKey: string }>()
const ctx = inject(MOD_GROUP_CTX)!
const pkg = computed(() => ctx.groups.find(group => group.key === props.groupKey)
  ?.packages.find(item => item.key === props.packageKey))
</script>
<template>
  <div v-if="pkg" class="package-body">
    <!--
      包内子 DLL 行不是独立单元：不加定位锚点，也不参与状态高亮。
      状态结论属于整个包，高亮只画在外壳上（见 package-shell）。
      外壳仍由 LocalItemCard 提供；这些行没有状态标签，
      所以不传 #status，组件会自己补 1px 隐形占位保持按钮列对齐。
      顺序是「主 MOD → 它带的依赖 DLL」：标题后带版本号，依赖项多一个
      灰色「随包依赖」标签（排序见 toPackage）。
      依赖 DLL 随包启停，独立禁用会让主 MOD 缺失运行库。
    -->
    <LocalItemCard v-for="mod in pkg.visibleMods" :key="mod.dll_file"
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
        状态标签统一在包标题行，子卡工具只处理当前 DLL。
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
          @click.stop="ctx.openLocation(mod)">
          <FolderOpenOutline width="14" height="14" />
        </n-button>
        <n-button size="tiny" quaternary type="error"
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
</template>
<style scoped>
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

.package-body .detail-section--table { margin: 0; padding: 0; border: 0; }
</style>
