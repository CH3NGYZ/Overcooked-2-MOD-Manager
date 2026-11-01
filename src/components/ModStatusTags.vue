<script setup lang="ts">
import { computed } from 'vue'
import { NTag } from 'naive-ui'
import { t } from '../i18n'
import { tagColorForAny } from '../theme'
import { useDashboard } from '../composables/useDashboardStore'
import { useRepositoryRefresh } from '../composables/useRepositoryRefresh'
import { useRepositoryLink } from '../crossLink'
import { installedModTags } from '../statusTags'
import { runtimeModStatus } from '../modRuntimeStatus'
import { relationJumpRepositoryId } from '../modRelations'
import { repositoryForMod } from '../modStatus'
import { tagTargetId, tagTargetTitle } from '../tagTargets'
import type { InstalledMod } from '../types'

/**
 * 一个已安装 MOD 的状态标签（MOD 管理页卡片用）。
 *
 * ## 为什么要做成组件、而不是在页面里 `v-for` 一遍
 *
 * 卡片挂在 `n-collapse-item` 里，而 naive-ui 的 `CollapseItemContent` 是拿
 * **稳定插槽**渲染内容的：父级重渲染时 Vue 的 `shouldUpdateComponent` 直接
 * 跳过它，插槽闭包不再求值。于是页面级的数据（`refreshing` 等）变了，页头
 * 的聚合标签会跟着变，卡片标签却停在上一轮结论 —— 用户点完「检查更新」，
 * 页头显示「4 刷新中」，卡片还印着「最新版」。
 *
 * 组件有自己的渲染效应：只要它自己读了 `refreshing` / `config.repositories`
 * 这些响应式来源，状态一变它就重渲染，与父级是否重渲染无关。
 *
 * ## 数据从哪来
 *
 * 只收一个 `dllFile` 当身份，**其余全部现读**：Dashboard 里的 MOD、仓库缓存、
 * 刷新状态。这样连「父级塞进来的 mod 是旧对象」都不会成为问题 ——
 * 判定逻辑（`modRuntimeStatus`）与页头聚合用的是同一份实现。
 */
const props = defineProps<{ dllFile: string }>()

const { mods, config } = useDashboard()
const { refreshing, queued, sourceRefreshing, entryStateUnavailable } = useRepositoryRefresh()
const { linkToRepository } = useRepositoryLink()

/** 始终按当前 Dashboard 取这条 MOD：单仓刷新只替换仓库缓存，这里也要跟上。 */
const mod = computed<InstalledMod | undefined>(() =>
  mods.value.find((entry) => entry.dll_file === props.dllFile))
const repositories = computed(() => config.value?.repositories ?? [])

const status = computed(() => {
  const current = mod.value
  if (!current) return undefined
  return runtimeModStatus(current, mods.value, repositories.value, {
    refreshing: refreshing.value,
    queued: queued.value,
    sourceRefreshing: sourceRefreshing.value,
    entryStateUnavailable: entryStateUnavailable.value,
  })
})

const tags = computed(() =>
  (status.value ? installedModTags(status.value.mod) : [])
    .map((tag) => ({ ...tag, text: t(tag.text) })))

/** 标签点了跳哪个仓库：依赖类跳依赖仓库，其余跳自己绑定的仓库。 */
function targetOf(tagKey: string): string | undefined {
  const current = mod.value
  if (!current) return undefined
  return tagTargetId(
    tagKey,
    repositoryForMod(repositories.value, current)?.id,
    relationJumpRepositoryId(status.value?.issue),
  )
}

function titleOf(tagKey: string) {
  return tagTargetTitle(tagKey, targetOf(tagKey))
}

function activate(tagKey: string) {
  const id = targetOf(tagKey)
  if (id) linkToRepository(id)
}
</script>

<template>
  <n-tag v-for="tag in tags" :key="tag.key" size="tiny" :bordered="false"
    class="state-tag" :class="{ 'state-tag--link': !!targetOf(tag.key) }"
    :color="tagColorForAny(tag.tone)"
    :title="titleOf(tag.key)"
    @click.stop="activate(tag.key)">{{ tag.text }}</n-tag>
</template>
