<script lang="ts">
/**
 * 依赖表里的一行。
 *
 * `dependencyRows.ts` 从同一份评估结果投影状态、原因和跳转入口；表格只展示。
 *
 * `kind` / `line` 是两个例外：它们是关系声明里的原始枚举，不是判定结果，
 * 表格统一翻译成文案，调用方不必各写一套映射。
 */
import type { DependencyRow } from '../dependencyRows'

/*
 * 行模型定义在 `dependencyRows.ts`：类型跟着构造逻辑走（目标名解析、状态
 * 文案、跳转目标都在那边算），这里 re-export 只为兼容既有调用点 ——
 * 「表格长什么样」和「一行是什么」不该分居两处。
 */
export type { DependencyJump, DependencyRow } from '../dependencyRows'
</script>

<script setup lang="ts">
/**
 * 依赖声明表 —— 发布详情面板（MOD / 地图）与地图管理页详情共用。
 *
 * 为什么是表格而不是一排标签：一张图/一个包可能声明三四条前置（DIYLevel、
 * Web Runtime、宿主版本…），每条都带「版本要求」（区间表达式原文）和「本地是否满足」。
 * 标签只能表达名字，满足情况得另起一行解释；表格把三件事对齐在同一列上，
 * 用户扫一眼就知道哪条需要处理、要求多少；原因与状态并排，窄列可换行。
 *
 * 只做版式：`rows` 为空时整块不渲染 —— 没有依赖声明的条目不该留一行
 * 「无依赖」，那会让详情面板多出一条没有信息量的分隔线。
 *
 * 表头就是标题（「MOD / 关系 / 要求 / 状态」），不再在表格上方重复一行
 * 「依赖」：用户看的是表格内容，标题只占高度。
 *
 * 状态列那枚标签与卡片上的状态标签是**同一档**（`.state-tag`，同一套
 * `theme.ts` 色阶），不另起一种小标签：同一件事在全站长得不一样，用户会
 * 以为它们是两回事。行带 `jump` 时标签变成可点标签并抛 `jump` 事件 ——
 * 组件不认识路由，「跳到哪个页面哪张卡片」由调用方决定。
 */
import { computed } from 'vue'
import { NTag } from 'naive-ui'
import { useDependencyNavigation } from '../composables/useDependencyNavigation'
import { t } from '../i18n'
import { tagColorForAny } from '../theme'

const props = withDefaults(defineProps<{
  rows?: DependencyRow[]
  /** 名称列是否用等宽字体（GUID 这类标识符）；仓库名/本地名保持正文字体。 */
  monoName?: boolean
}>(), {
  rows: () => [],
  monoName: false,
})

const visible = computed(() => props.rows.length > 0)
/** 状态标签被点击：只有带 `jump` 的行会发，调用方据此跳转。 */
const emit = defineEmits<{ jump: [row: DependencyRow] }>()
const navigation = useDependencyNavigation(row => emit('jump', row))

/** 可操作标签保持详情展开；确认与陈旧目标校验由共享导航流程负责。 */
function clickStatus(row: DependencyRow, event?: Event) {
  if (!row.jump) return
  event?.stopPropagation()
  navigation.request(() => props.rows.find(current => current.key === row.key))
}

/**
 * 关系列只在出现「不止依赖」的关系时出现。
 *
 * 地图的 `required_frameworks` 投影出来全是 `requires`，MOD 的多数仓库也
 * 只声明依赖；这种情况下多一列同样的字只会占宽度。出现冲突/兼容声明、
 * 或者关系限定了发布线时才需要这列来区分含义。
 */
const showKind = computed(() =>
  props.rows.some((row) => !!row.line || (!!row.kind && row.kind !== 'requires')))

/** 关系列文案：依赖 / 排斥 / 兼容；没有关系信息时用 `-` 占位。 */
function kindLabel(row: DependencyRow): string {
  if (!row.kind) return '-'
  if (row.kind === 'conflicts') return t('tools.relationConflicts')
  if (row.kind === 'compatible') return t('tools.relationCompatible')
  return t('tools.relationRequires')
}

/** 发布线小标；关系没限定发布线时不渲染。 */
function lineLabel(line: DependencyRow['line']): string {
  return line === 'prerelease' ? t('tools.testLine') : t('tools.stableLine')
}
</script>

<template>
  <section v-if="visible" class="dep-block">
    <div class="dep-scroll oc2-table-scroll">
      <table class="dep-table oc2-detail-table">
        <thead>
          <tr>
            <th scope="col">{{ t('tools.modColumn') }}</th>
            <th v-if="showKind" scope="col" class="oc2-table-fit">{{ t('tools.relation') }}</th>
            <th scope="col" class="oc2-table-fit">{{ t('tools.requirement') }}</th>
            <th scope="col" class="oc2-table-fit">{{ t('tools.status') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.key">
            <td class="dep-name" :class="{ mono: monoName }" :title="row.name">{{ row.name || '-' }}</td>
            <td v-if="showKind" class="dep-kind">
              {{ kindLabel(row) }}
              <span v-if="row.line" class="dep-line">{{ lineLabel(row.line) }}</span>
            </td>
            <td class="dep-req oc2-table-version mono">{{ row.requirement || '-' }}</td>
            <td class="dep-state">
              <div v-if="row.status" class="dep-verdict">
                <n-tag size="tiny" :bordered="false" class="state-tag"
                  :class="{ 'state-tag--link': !!row.jump }" :color="tagColorForAny(row.tone ?? 'muted')"
                  :title="row.jump?.title" :role="row.jump ? 'button' : undefined"
                  :tabindex="row.jump ? 0 : undefined"
                  @click="clickStatus(row, $event)" @keydown.enter="clickStatus(row, $event)"
                  @keydown.space.prevent="clickStatus(row, $event)">
                  {{ row.status }}
                </n-tag>
                <span v-if="row.reason" class="dep-reason">{{ row.reason }}</span>
              </div>
              <span v-else>—</span>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<style scoped>
.dep-block {
  min-inline-size: 0;
  container-type: inline-size;
}

.dep-table {
  color: var(--oc2-text-2);
}

.dep-name {
  color: var(--oc2-text-1);
}

.dep-name,
.dep-reason {
  overflow-wrap: anywhere;
}

.dep-kind,
.dep-req {
  color: var(--oc2-text-3);
}

/* 发布线小标：跟在关系名后面，说明这条关系指的是测试版还是正式版 */
.dep-line {
  display: inline-block;
  margin-left: 6px;
  padding: 0 6px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 999px;
  background: var(--oc2-sunken);
  font-size: 11px;
  color: var(--oc2-text-3);
  white-space: nowrap;
}

.dep-verdict {
  /* 按标签和说明预留宽度，说明再长也给名称及版本留空间。 */
  inline-size: max-content;
  min-inline-size: min-content;
  max-inline-size: min(32ch, 30cqi);
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 3px 6px;
}

.dep-verdict .state-tag { flex: none; }

.dep-reason {
  color: var(--oc2-text-3);
}

/* 可点标签的手型与悬停反馈由 global.css 的 .state-tag--link 统一提供 */
</style>
