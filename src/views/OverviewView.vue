<script setup lang="ts">
import { t } from '../i18n'
import { onMounted } from 'vue'
import { NSpin } from 'naive-ui'
import StatCard from '../components/StatCard.vue'
import { useDashboard } from '../composables/useDashboardStore'

const { load, loading, error, mods, enabledModCount, updateCount, frameworkOk, game } = useDashboard()

onMounted(() => { void load() })

const pathVerified = () => !!game.value?.path && game.value.game_exists
</script>

<template>
  <n-spin :show="loading">
    <div v-if="error" class="muted">{{ error }}</div>
    <div v-else class="card-grid">
      <StatCard :label="t('已安装 MOD')" :value="String(mods.length)"
        :sub="t('已启用 {count} 个', { count: enabledModCount })"
        :tone="mods.length ? 'success' : 'default'" />
      <StatCard :label="t('框架状态')" :value="frameworkOk ? t('正常') : t('未安装')"
        :sub="game?.bepinex.version ? `BepInEx ${game.bepinex.version}` : t('未安装')"
        :tone="frameworkOk ? 'success' : 'warning'" />
      <StatCard :label="t('游戏路径')" :value="pathVerified() ? t('已验证') : (game?.path ? t('目录异常') : t('未设置'))"
        :sub="game?.path || t('尚未检测到游戏目录')"
        :tone="pathVerified() ? 'success' : 'warning'" />
      <StatCard :label="t('待处理事项')" :value="`${updateCount} 项`"
        :sub="updateCount ? t('有可更新的 MOD') : t('暂无可更新内容')"
        :tone="updateCount ? 'warning' : 'success'" />
    </div>
  </n-spin>
</template>
