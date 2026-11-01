<script setup lang="ts">
import { computed } from 'vue'
import { NTag } from 'naive-ui'
import { locale } from '../i18n'
import { tagColorFor } from '../theme'
import {
  describeRefreshOutcome, describeRefreshOutcomeTitle, type RefreshDomain,
} from '../refreshOutcomeText'
import type { RefreshOutcome } from '../refreshState'

/**
 * 「上次检查」摘要标签（MOD 管理页与地图管理页共用）。
 *
 * 为什么要有它：刷新结果原来只以 toast 出现，一闪就没了 —— 用户过几分钟
 * 再看列表，没法判断「我看到的版本结论是刚刷出来的，还是昨晚的」。这枚标签
 * 把最近一次批次的结果常驻在页头，悬停能看到完成时间与失败原因。
 *
 * 文案来自 `refreshOutcomeText`，与刷新完成时那条 toast 用的是同一个函数，
 * 两处永远说同一句话。
 */
const props = defineProps<{
  /** 最近一次批次结果；`null` 表示这次会话还没刷过。 */
  outcome: RefreshOutcome | null
  /** 条目量词：仓库（个）还是地图（张）。 */
  domain: RefreshDomain
}>()

const text = computed(() =>
  (props.outcome ? describeRefreshOutcome(props.outcome, props.domain) : ''))
const title = computed(() =>
  (props.outcome ? describeRefreshOutcomeTitle(props.outcome, locale.value) : ''))
/** 有失败时升一档颜色：这是需要用户注意的信息，不跟「都最新」一个灰。 */
const tone = computed(() => (props.outcome?.errors.length ? 'moderate' as const : 'muted' as const))
</script>

<template>
  <n-tag v-if="outcome" size="small" :bordered="false" class="state-tag refresh-result"
    :color="tagColorFor(tone)" :title="title">
    {{ text }}
  </n-tag>
</template>

<style scoped>
/* 摘要要能一眼读完，不要在窄窗口里被挤成两行 */
.refresh-result { white-space: nowrap; }
</style>
