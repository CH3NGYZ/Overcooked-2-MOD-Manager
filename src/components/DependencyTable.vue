<script lang="ts">
/**
 * 依赖表里的一行。
 *
 * 行的**内容由调用方算好**：面板/页面才知道「已满足」「缺依赖」是由哪条判定
 * 得出的（地图看 `required_frameworks` + 本地 MOD，MOD 看 `relations` +
 * Release），把这个判定塞进表格组件等于让组件认识两种业务模型。
 *
 * `kind` / `line` 是两个例外：它们是关系声明里的原始枚举，不是判定结果，
 * 表格统一翻译成文案，调用方不必各写一套映射。
 */
import type { RelationKind } from '../types'

export interface DependencyRow {
  /** 稳定 key（GUID / 仓库 id），用于 `v-for`。 */
  key: string
  /** 依赖目标名：仓库名、本地 MOD 名，或解析不出名字时的 GUID。 */
  name: string
  /** 关系种类；省略时按 `requires` 处理。 */
  kind?: RelationKind
  /**
   * 关系限定的发布线（来自关系的 `prerelease` 三态）。
   *
   * `conflicts` 用它区分「排斥某个版本区间」与「和另一条发布线互斥」，
   * 表格里额外标出「测试版 / 正式版」，用户不用去翻仓库配置才知道指的是哪条线。
   */
  line?: 'prerelease' | 'stable'
  /** 版本要求文本（`≥ 1.2.0`、`1.0 ~ 2.0`）；空表示不限版本。 */
  requirement?: string
  /** 状态文案（已满足 / 缺依赖 / 冲突）。 */
  status: string
  /** 状态色调，走 `theme.ts` 的统一刻度。 */
  tone?: 'accent' | 'mild' | 'moderate' | 'severe' | 'critical' | 'muted'
}
</script>

<script setup lang="ts">
/**
 * 依赖声明表 —— 发布详情面板（MOD / 地图）与地图管理页详情共用。
 *
 * 为什么是表格而不是一排标签：一张图/一个包可能声明三四条前置（DIYLevel、
 * Web Runtime、宿主版本…），每条都带「要求的最低版本」和「本地是否满足」。
 * 标签只能表达名字，满足情况得另起一行解释；表格把三件事对齐在同一列上，
 * 用户扫一眼就知道缺哪条、要求多少。
 *
 * 只做版式：`rows` 为空时整块不渲染 —— 没有依赖声明的条目不该留一行
 * 「无依赖」，那会让详情面板多出一条没有信息量的分隔线。
 *
 * 表头就是标题（「MOD / 关系 / 要求 / 状态」），不再在表格上方重复一行
 * 「依赖」：用户看的是表格内容，标题只占高度。
 */
import { computed } from 'vue'
import { NTag } from 'naive-ui'
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
    <div class="dep-scroll">
      <table class="dep-table">
        <thead>
          <tr>
            <th class="dep-col-name">{{ t('tools.modColumn') }}</th>
            <th v-if="showKind" class="dep-col-kind">{{ t('tools.relation') }}</th>
            <th class="dep-col-req">{{ t('tools.requirement') }}</th>
            <th class="dep-col-state">{{ t('tools.status') }}</th>
          </tr>
        </thead>
        <tbody>
          <tr v-for="row in rows" :key="row.key">
            <td class="dep-name" :class="{ mono: monoName }" :title="row.name">{{ row.name || '-' }}</td>
            <td v-if="showKind" class="dep-kind">
              {{ kindLabel(row) }}
              <span v-if="row.line" class="dep-line">{{ lineLabel(row.line) }}</span>
            </td>
            <td class="dep-req mono">{{ row.requirement || '-' }}</td>
            <td class="dep-state">
              <n-tag size="tiny" :bordered="false" :color="tagColorForAny(row.tone ?? 'muted')">
                {{ row.status }}
              </n-tag>
            </td>
          </tr>
        </tbody>
      </table>
    </div>
  </section>
</template>

<style scoped>
.dep-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

/* 长仓库名/长 GUID 不该把面板撑宽：超出时表格内部横向滚动 */
.dep-scroll {
  overflow-x: auto;
  border: 1px solid var(--oc2-row-line);
  border-radius: 10px;
  background: var(--oc2-card);
}

.dep-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
  line-height: 1.5;
}

.dep-table th {
  padding: 6px 10px;
  text-align: left;
  font-weight: 600;
  white-space: nowrap;
  color: var(--oc2-text-3);
  background: var(--oc2-sunken);
  border-bottom: 1px solid var(--oc2-row-line);
}

.dep-table td {
  padding: 6px 10px;
  color: var(--oc2-text-2);
  border-bottom: 1px solid var(--oc2-row-line);
  vertical-align: middle;
}

/* 最后一行不画线：外框已经有边框，再画一条会显得厚 */
.dep-table tbody tr:last-child td {
  border-bottom: 0;
}

.dep-col-name { width: 44%; }
.dep-col-kind { width: 20%; }
.dep-col-req { width: 18%; }
.dep-col-state { width: 18%; }

.dep-name {
  min-width: 0;
  max-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--oc2-text-1);
}

.dep-kind,
.dep-req {
  white-space: nowrap;
  color: var(--oc2-text-3);
}

/* 发布线小标：跟在关系名后面，说明这条关系指的是测试版还是正式版 */
.dep-line {
  margin-left: 6px;
  padding: 0 6px;
  border: 1px solid var(--oc2-row-line);
  border-radius: 999px;
  background: var(--oc2-sunken);
  font-size: 11px;
  color: var(--oc2-text-3);
}

.dep-state {
  white-space: nowrap;
}
</style>
