<script setup lang="ts">
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
      <StatCard :label="'已安装 MOD'" :value="String(mods.length)"
        :sub="`已启用 ${enabledModCount} 个`"
        :tone="mods.length ? 'success' : 'default'" />
      <StatCard :label="'框架状态'" :value="frameworkOk ? '正常' : '未安装'"
        :sub="game?.bepinex.version ? `BepInEx ${game.bepinex.version}` : 'BepInEx 未安装'"
        :tone="frameworkOk ? 'success' : 'warning'" />
      <StatCard :label="'游戏路径'" :value="pathVerified() ? '已验证' : (game?.path ? '目录异常' : '未设置')"
        :sub="game?.path || '尚未检测到游戏目录'"
        :tone="pathVerified() ? 'success' : 'warning'" />
      <StatCard :label="'待处理事项'" :value="`${updateCount} 项`"
        :sub="updateCount ? '有可更新的 MOD' : '暂无可更新内容'"
        :tone="updateCount ? 'warning' : 'success'" />
    </div>
  </n-spin>
</template>
