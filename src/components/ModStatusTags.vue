<script setup lang="ts">
import { computed } from 'vue'
import { NTag } from 'naive-ui'
import { pickI18n, t } from '../i18n'
import { tagColorForAny } from '../theme'
import { useDashboard } from '../composables/useDashboardStore'
import { useModRepositoryRefresh } from '../composables/useModRepositoryRefresh'
import { useModRepositoryLink } from '../crossLink'
import { installedModTags } from '../statusTags'
import { runtimeModStatus } from '../modRuntimeStatus'
import { isRelationCardKey } from '../modRelations'
import { updateRepositoryForMod } from '../modStatus'
import { dependencyRowFromStatus, dependencyTagTitle } from '../dependencyRows'
import { useDependencyNavigation } from '../composables/useDependencyNavigation'
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
const { runtimeState } = useModRepositoryRefresh()
const { linkToModRepository } = useModRepositoryLink()
const dependencyNavigation = useDependencyNavigation(row => {
  if (row.jump) linkToModRepository(row.jump.id, row.jump.targetVersion)
})

/** 始终按当前 Dashboard 取这条 MOD：单仓刷新只替换仓库缓存，这里也要跟上。 */
const mod = computed<InstalledMod | undefined>(() =>
  mods.value.find((entry) => entry.dll_file === props.dllFile))
const repositories = computed(() => config.value?.repositories ?? [])

const status = computed(() => {
  const current = mod.value
  if (!current) return undefined
  return runtimeModStatus(current, mods.value, repositories.value, runtimeState.value)
})

function relationRow(tagKey: string) {
  const summary = status.value?.relationTag
  if (!summary || summary.key !== tagKey) return undefined
  return dependencyRowFromStatus(summary.dependency, { mods: mods.value, repositories: repositories.value,
    translate: t, pickName: pickI18n, jumpToRepository: true })
}

const repositoryId = computed(() => {
  const current = mod.value
  return current && updateRepositoryForMod(repositories.value, current)?.id
})

const tags = computed(() => (status.value ? installedModTags(status.value.mod) : []).map(tag => {
  const row = relationRow(tag.key)
  const clickable = isRelationCardKey(tag.key) ? !!row?.jump : !!repositoryId.value
  const title = row ? dependencyTagTitle(row) : clickable ? t('dialogs.viewInModInstall') : undefined
  return { ...tag, text: t(tag.text), title, clickable }
}))

function activate(tagKey: string) {
  if (isRelationCardKey(tagKey)) {
    dependencyNavigation.request(() => relationRow(tagKey))
    return
  }
  const id = repositoryId.value
  if (id) linkToModRepository(id)
}
</script>

<template>
  <n-tag v-for="tag in tags" :key="tag.key" size="tiny" :bordered="false"
    class="state-tag" :class="{ 'state-tag--link': tag.clickable }"
    :color="tagColorForAny(tag.tone)"
    :title="tag.title"
    @click.stop="activate(tag.key)">{{ tag.text }}</n-tag>
</template>
